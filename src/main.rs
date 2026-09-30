//! L4 TCP load balancer: accept → pick backend (round-robin) → connect → copy bytes.
//!
//!   tiny-lb 0.0.0.0:8080 127.0.0.1:8001 127.0.0.1:8002
//!   tiny-lb echo 8001

use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let first = args.next().unwrap_or_else(|| {
        eprintln!("usage:\n  tiny-lb <listen> <backend> [backend...]\n  tiny-lb echo <port>");
        std::process::exit(1);
    });

    if first == "echo" {
        let port: u16 = args.next().expect("echo port").parse().unwrap();
        echo(port).await.unwrap();
        return;
    }

    let listen: SocketAddr = first.parse().expect("listen addr");
    let backends: Vec<SocketAddr> = args.map(|s| s.parse().expect("backend addr")).collect();
    if backends.is_empty() {
        eprintln!("need at least one backend");
        std::process::exit(1);
    }
    serve(listen, backends).await.unwrap();
}

async fn serve(listen: SocketAddr, backends: Vec<SocketAddr>) -> io::Result<()> {
    let listener = TcpListener::bind(listen).await?;
    let backends = Arc::new(backends);
    let rr = Arc::new(AtomicUsize::new(0));
    eprintln!("listen {listen}");
    loop {
        let (mut client, peer) = listener.accept().await?;
        let backends = Arc::clone(&backends);
        let rr = Arc::clone(&rr);
        tokio::spawn(async move {
            let n = backends.len();
            let start = rr.fetch_add(1, Ordering::Relaxed);
            for off in 0..n {
                let addr = backends[(start + off) % n];
                match TcpStream::connect(addr).await {
                    Ok(mut backend) => {
                        eprintln!("{peer} -> {addr}");
                        let _ = copy_bidirectional(&mut client, &mut backend).await;
                        return;
                    }
                    Err(e) => eprintln!("{peer} -> {addr} failed: {e}"),
                }
            }
        });
    }
}

async fn echo(port: u16) -> io::Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    eprintln!("echo 0.0.0.0:{port}");
    loop {
        let (mut s, _) = listener.accept().await?;
        tokio::spawn(async move {
            let (mut r, mut w) = s.split();
            let _ = tokio::io::copy(&mut r, &mut w).await;
        });
    }
}
