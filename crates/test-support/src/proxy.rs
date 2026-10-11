//! TCP proxy that adds one-way delay and jitter in both directions while
//! preserving byte order, for exercising the client and server under
//! realistic WAN conditions.

use std::net::SocketAddr;
use std::time::Duration;

use rand::Rng;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::Instant;

pub struct LatencyProxy {
    pub addr: SocketAddr,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for LatencyProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl LatencyProxy {
    pub async fn start(upstream: SocketAddr, one_way: Duration, jitter: Duration) -> std::io::Result<LatencyProxy> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let task = tokio::spawn(async move {
            while let Ok((client, _)) = listener.accept().await {
                let Ok(server) = TcpStream::connect(upstream).await else { continue };
                let (cr, cw) = client.into_split();
                let (sr, sw) = server.into_split();
                tokio::spawn(pipe(cr, sw, one_way, jitter));
                tokio::spawn(pipe(sr, cw, one_way, jitter));
            }
        });
        Ok(LatencyProxy { addr, task })
    }
}

async fn pipe(
    mut from: tokio::net::tcp::OwnedReadHalf,
    mut to: tokio::net::tcp::OwnedWriteHalf,
    delay: Duration,
    jitter: Duration,
) {
    let (tx, mut rx) = mpsc::unbounded_channel::<(Instant, Vec<u8>)>();
    let writer = tokio::spawn(async move {
        while let Some((at, chunk)) = rx.recv().await {
            tokio::time::sleep_until(at).await;
            if to.write_all(&chunk).await.is_err() {
                break;
            }
        }
        let _ = to.shutdown().await;
    });
    let mut last = Instant::now();
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        let n = match from.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let extra =
            if jitter.is_zero() { Duration::ZERO } else { rand::thread_rng().gen_range(Duration::ZERO..jitter) };
        // TCP is ordered: a chunk can never overtake the one before it.
        let at = (Instant::now() + delay + extra).max(last);
        last = at;
        if tx.send((at, buf[..n].to_vec())).is_err() {
            break;
        }
    }
    drop(tx);
    let _ = writer.await;
}
