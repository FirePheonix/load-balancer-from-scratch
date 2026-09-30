use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Open `n` concurrent connections to the LB and keep them alive so the UI can show scale.
pub async fn run(target: SocketAddr, n: usize, hold: Duration) -> io::Result<()> {
    eprintln!("load {n} conns -> {target} for {hold:?}");
    let mut joins = Vec::with_capacity(n);
    let deadline = Instant::now() + hold;
    for _ in 0..n {
        joins.push(tokio::spawn(async move {
            let mut s = TcpStream::connect(target).await?;
            while Instant::now() < deadline {
                s.write_all(b".").await?;
                let mut one = [0u8; 1];
                s.read_exact(&mut one).await?;
                tokio::time::sleep(Duration::from_millis(400)).await;
            }
            Ok::<_, io::Error>(())
        }));
    }
    let mut ok = 0usize;
    let mut err = 0usize;
    for j in joins {
        match j.await {
            Ok(Ok(())) => ok += 1,
            _ => err += 1,
        }
    }
    eprintln!("load done ok={ok} err={err}");
    Ok(())
}
