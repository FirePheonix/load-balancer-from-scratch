//! L4 TCP load balancer: accept → pick backend (round-robin) → connect → copy bytes.
//!
//!   tiny-lb 0.0.0.0:8080 127.0.0.1:8001 127.0.0.1:8002

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
        eprintln!("usage: tiny-lb <listen> <backend> [backend...]");
        std::process::exit(1);
    });
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
            let i = rr.fetch_add(1, Ordering::Relaxed) % backends.len();
            let addr = backends[i];
            match TcpStream::connect(addr).await {
                Ok(mut backend) => {
                    eprintln!("{peer} -> {addr}");
                    let _ = copy_bidirectional(&mut client, &mut backend).await;
                }
                Err(e) => eprintln!("{peer} -> {addr} failed: {e}"),
            }
        });
    }
}
