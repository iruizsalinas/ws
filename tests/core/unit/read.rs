use super::*;
use crate::net::link::Config;
use crate::net::link::handle::INBOX_LIMIT;
use crate::net::runtime;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::mpsc::channel;
use std::time::Duration;

#[test]
fn closing_connections_do_not_wait_for_application_consumption() {
    let (events, _rx) = channel();
    let link = Link::new(1, true, Config::test(1024), events.into());
    link.inbox.store(INBOX_LIMIT, Relaxed);
    lock(&link.flags).state = State::Closing;
    assert!(link.wait_inbox(State::Closing), "backpressure must not prevent reading the peer's close reply");
}

#[test]
fn cancelled_start_closes_the_transport_even_when_the_handle_is_retained() {
    use std::io::Read;
    use std::net::{TcpListener, TcpStream};

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
    let (tcp, _) = listener.accept().unwrap();
    tcp.set_nonblocking(true).unwrap();
    let (events, _rx) = channel();
    let link = Link::new(1, false, Config::test(1024), events.into());
    link.terminate();
    runtime::shared().unwrap().block_on(async {
        let tcp = tokio::net::TcpStream::from_std(tcp).unwrap();
        let stream = crate::net::transport::server(tcp, None).await.unwrap();
        link.start(stream, vec![], None, Event::Open { id: 1 }).await;
    });
    assert_eq!(peer.read(&mut [0]).expect("cancelled connection retained a live socket"), 0);
    assert_eq!(link.state(), State::Closed);
}

#[test]
fn aborted_connection_task_closes_a_retained_open_socket() {
    use std::io::Read;
    use std::net::{TcpListener, TcpStream};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let (tcp, _) = listener.accept().unwrap();
    tcp.set_nonblocking(true).unwrap();
    let (events, rx) = channel();
    let link = Link::new(1, false, Config::test(1024), events.into());
    runtime::shared().unwrap().block_on(async {
        let tcp = tokio::net::TcpStream::from_std(tcp).unwrap();
        let stream = crate::net::transport::server(tcp, None).await.unwrap();
        let worker = link.clone();
        let task = tokio::spawn(async move {
            worker.start(stream, vec![], None, Event::Open { id: 1 }).await;
        });
        assert!(matches!(rx.recv_timeout(Duration::from_secs(1)).unwrap().event, Event::Open { .. }));
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
    });
    assert_eq!(link.state(), State::Closed);
    assert_eq!(peer.read(&mut [0]).unwrap(), 0);
    assert!(matches!(rx.recv_timeout(Duration::from_secs(1)).unwrap().event, Event::Close { code: 1006, .. }));
    assert!(rx.try_recv().is_err(), "only one final event");
}
