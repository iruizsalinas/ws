//! Staggered address attempts. Dropping the futures cancels every losing socket.
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::task::JoinSet;

const STAGGER: Duration = Duration::from_millis(250);

pub(super) async fn dial(addrs: Vec<SocketAddr>) -> Result<TcpStream, Option<String>> {
    let mut attempts = JoinSet::new();
    let mut addresses = addrs.into_iter();
    let mut error = None;
    let start = |tasks: &mut JoinSet<_>, addr| {
        tasks
            .spawn(async move { TcpStream::connect(addr).await.map_err(|e| format!("cannot connect to {addr}: {e}")) });
    };
    if let Some(addr) = addresses.next() {
        start(&mut attempts, addr);
    }
    while !attempts.is_empty() {
        tokio::select! {
            result = attempts.join_next() => match result {
                Some(Ok(Ok(tcp))) => return Ok(tcp),
                Some(Ok(Err(e))) => {
                    error = Some(e);
                    if let Some(addr) = addresses.next() { start(&mut attempts, addr); }
                }
                Some(Err(e)) => error = Some(format!("cannot connect: {e}")),
                None => break,
            },
            _ = tokio::time::sleep(STAGGER) => {
                if let Some(addr) = addresses.next() { start(&mut attempts, addr); }
            }
        }
    }
    Err(error)
}
