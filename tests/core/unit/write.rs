use super::*;
use crate::net::activity::{Activity, Observed};
use crate::net::link::heartbeat::{Alive, Probe};
use crate::net::link::{Config, State};
use crate::net::runtime;
use std::sync::Mutex;
use std::sync::mpsc::channel;
use tokio::io::AsyncReadExt;

#[test]
fn a_heartbeat_waiting_for_socket_capacity_gets_a_full_response_interval() {
    runtime::shared().unwrap().block_on(async {
        let interval = Duration::from_millis(50);
        let (events, _rx) = channel();
        let link = Link::new(1, true, Config { heartbeat: Some(interval), ..Config::test(1024) }, events.into());
        lock(&link.flags).state = State::Open;
        let (mut writer, mut reader) = tokio::io::duplex(14);
        writer.write_all(&[0; 14]).await.unwrap(); // full socket before the probe
        let activity = Arc::new(Activity::default());
        activity.bind(link.born, link.written.clone(), link.received.clone());
        let mut writer = Observed { io: writer, activity };
        let data = link.frame(crate::proto::frame::PING, &link.token);
        link.buffered.store(data.len(), Relaxed);
        let sent = Arc::new(Mutex::new(None));
        let queued = std::time::Instant::now();
        let peer = tokio::spawn(async move {
            tokio::time::sleep(interval * 3).await;
            let mut dummy = [1; 14];
            reader.read_exact(&mut dummy).await.unwrap();
            assert_eq!(dummy, [0; 14]);
            let mut probe = vec![0; 10];
            reader.read_exact(&mut probe).await.unwrap();
            probe
        });
        link.write_out(&mut writer, &data, Some(&sent), &mut Budget::default(), 16 * 1024).await.unwrap();
        let mut alive = Alive { seen: queued, ping: Some(Probe { queued, sent }) };
        assert!(link.heartbeat(&mut alive), "a queued probe inherited an expired response deadline");
        assert_eq!(link.state(), State::Open);
        assert_eq!(peer.await.unwrap(), data);
    });
}
