use crate::state::{State, CONNECT_TIMEOUT_MS, IDLE_TIMEOUT_MS, MAX_CONNS};
use std::io;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

pub async fn run(state: Arc<State>) -> io::Result<()> {
    let listener = TcpListener::bind(state.listen).await?;
    eprintln!("data plane {}  admin http://{}", state.listen, state.admin);
    loop {
        let (client, peer) = listener.accept().await?;
        if state.active.load(Ordering::Relaxed) >= MAX_CONNS {
            state.rejected.fetch_add(1, Ordering::Relaxed);
            drop(client);
            continue;
        }
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(e) = handle(client, peer, state).await {
                eprintln!("{peer}: {e}");
            }
        });
    }
}

async fn handle(mut client: TcpStream, peer: SocketAddr, state: Arc<State>) -> io::Result<()> {
    state.opened.fetch_add(1, Ordering::Relaxed);
    let Some(start) = state.pick() else {
        state.failed.fetch_add(1, Ordering::Relaxed);
        return Ok(());
    };
    let n = state.backends.len();
    let mut backend_stream = None;
    let mut idx = None;
    for off in 0..n {
        let i = (start + off) % n;
        let b = &state.backends[i];
        if !b.can_accept() {
            continue;
        }
        match timeout(
            Duration::from_millis(CONNECT_TIMEOUT_MS),
            TcpStream::connect(b.addr),
        )
        .await
        {
            Ok(Ok(s)) => {
                backend_stream = Some(s);
                idx = Some(i);
                break;
            }
            _ => {
                b.connect_fail.fetch_add(1, Ordering::Relaxed);
                b.healthy.store(false, Ordering::Relaxed);
            }
        }
    }

    let (mut backend, i) = match (backend_stream, idx) {
        (Some(s), Some(i)) => (s, i),
        _ => {
            state.failed.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }
    };

    let b = &state.backends[i];
    eprintln!("{peer} -> {}", b.addr);
    b.active.fetch_add(1, Ordering::Relaxed);
    b.total.fetch_add(1, Ordering::Relaxed);
    state.active.fetch_add(1, Ordering::Relaxed);

    let result = splice(&mut client, &mut backend, &state).await;

    b.active.fetch_sub(1, Ordering::Relaxed);
    state.active.fetch_sub(1, Ordering::Relaxed);
    result.map(|_| ())
}

/// Bounded-buffer copy. Stops if either side is idle too long. Honors TCP half-close.
async fn splice(a: &mut TcpStream, b: &mut TcpStream, state: &State) -> io::Result<()> {
    let idle = Duration::from_millis(IDLE_TIMEOUT_MS);
    let (mut ar, mut aw) = a.split();
    let (mut br, mut bw) = b.split();
    let mut ab = [0u8; 16 * 1024];
    let mut bb = [0u8; 16 * 1024];
    let mut a_eof = false;
    let mut b_eof = false;
    while !a_eof || !b_eof {
        tokio::select! {
            r = timeout(idle, ar.read(&mut ab)), if !a_eof => {
                let n = r.map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "idle"))??;
                if n == 0 {
                    a_eof = true;
                    let _ = bw.shutdown().await;
                } else {
                    bw.write_all(&ab[..n]).await?;
                    state.bytes.fetch_add(n as u64, Ordering::Relaxed);
                }
            }
            r = timeout(idle, br.read(&mut bb)), if !b_eof => {
                let n = r.map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "idle"))??;
                if n == 0 {
                    b_eof = true;
                    let _ = aw.shutdown().await;
                } else {
                    aw.write_all(&bb[..n]).await?;
                    state.bytes.fetch_add(n as u64, Ordering::Relaxed);
                }
            }
        }
    }
    Ok(())
}
