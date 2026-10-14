use super::*;

#[test]
fn pending_pongs_are_bounded_and_keep_the_latest_payload() {
    use crate::net::link::Io;
    use crate::proto::reader::Reader;
    use std::net::{TcpListener, TcpStream};

    for server in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let _peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (tcp, _) = listener.accept().unwrap();
        let (tx, rx) = channel();
        let link = Link::new(1, server, Config::test(1024), tx.into());
        assert!(link.io.set(Io::new(tcp)).is_ok());
        lock(&link.flags).state = crate::net::link::State::Open;
        // Model a full writer batch. Pings must not allocate more encoded frames.
        link.buffered.store(1 << 20, Relaxed);
        for n in 0..2000u16 {
            let data = n.to_be_bytes().repeat(62); // near the control payload limit
            link.handle(Msg::Ping(data.clone()));
            assert!(matches!(rx.try_recv().map(|q| q.event), Ok(Event::Ping { .. })));
            link.consumed(data.len());
        }
        {
            let q = lock(&link.queue);
            assert!(q.items.is_empty(), "only one unsealed reply may wait for the writer");
            assert_eq!(q.pong.as_deref(), Some(1999u16.to_be_bytes().repeat(62).as_slice()));
        }
        assert_eq!(link.buffered.load(Relaxed), 1 << 20);
        link.flush_pong();
        let q = lock(&link.queue);
        assert!(q.pong.is_none());
        assert_eq!(q.items.len(), 1);
        let Item::Data(data) = &q.items[0] else { panic!("expected a pong frame") };
        let mut reader = Reader::new(!server, 1024);
        reader.push(data);
        assert!(matches!(reader.message().unwrap(), Some(Msg::Pong(data)) if data == 1999u16.to_be_bytes().repeat(62)));
        assert_eq!(reader.message().unwrap(), None);
    }
}

#[test]
fn deferred_pongs_are_discarded_on_close_or_protocol_failure() {
    for failed in [false, true] {
        let (tx, _rx) = channel();
        let link = Link::new(1, true, Config::test(1024), tx.into());
        lock(&link.flags).state = crate::net::link::State::Open;
        link.buffered.store(1 << 20, Relaxed);
        link.handle(Msg::Ping(b"pending".to_vec()));
        assert!(lock(&link.queue).pong.is_some());
        if failed {
            link.violation(Fail(1002, "bad frame"));
        } else {
            link.closing(Some((1000, String::new())));
        }
        link.flush_pong();
        let q = lock(&link.queue);
        assert!(q.pong.is_none());
        assert!(q.items.is_empty(), "no transport installed, and no pong may be sent");
    }
}
