use super::*;
use std::io::Read;
use std::net::TcpListener;
use std::time::Duration;

#[test]
fn shutdown_releases_its_socket_descriptor_even_if_the_handle_is_retained() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let (tcp, _) = listener.accept().unwrap();
    #[cfg(target_os = "linux")]
    let descriptor = {
        use std::os::fd::AsRawFd;
        let path = format!("/proc/self/fd/{}", tcp.as_raw_fd());
        let inode = std::fs::read_link(&path).unwrap();
        (path, inode)
    };
    let handle = Io::new(tcp);
    handle.shutdown();
    handle.shutdown();
    assert_eq!(peer.read(&mut [0]).unwrap(), 0);
    #[cfg(target_os = "linux")]
    assert_ne!(std::fs::read_link(&descriptor.0).ok(), Some(descriptor.1), "the shutdown descriptor leaked");
    drop(handle);
}
