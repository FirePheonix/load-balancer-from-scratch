//! L4 TCP load balancer.
//!
//!   tiny-lb 0.0.0.0:8080 127.0.0.1:8001 127.0.0.1:8002
//!   tiny-lb echo 8001
//!   tiny-lb load 127.0.0.1:8080 500

mod admin;
mod health;
mod load;
mod proxy;
mod state;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let first = args.next().unwrap_or_else(|| {
        usage();
        std::process::exit(1);
    });

    match first.as_str() {
        "echo" => {
            let port: u16 = args.next().expect("echo port").parse().unwrap();
            echo(port).await.unwrap();
        }
        "load" => {
            let target: SocketAddr = args.next().expect("target").parse().unwrap();
            let n: usize = args.next().unwrap_or_else(|| "200".into()).parse().unwrap();
            let secs: u64 = args.next().unwrap_or_else(|| "20".into()).parse().unwrap();
            load::run(target, n, Duration::from_secs(secs)).await.unwrap();
        }
        _ => {
            let listen: SocketAddr = first.parse().expect("listen addr");
            let backends: Vec<SocketAddr> =
                args.map(|s| s.parse().expect("backend addr")).collect();
            if backends.is_empty() {
                eprintln!("need at least one backend");
                std::process::exit(1);
            }
            let state = Arc::new(state::State::new(listen, backends));
            health::spawn(Arc::clone(&state));
            let admin = {
                let s = Arc::clone(&state);
                tokio::spawn(async move {
                    if let Err(e) = admin::run(s).await {
                        eprintln!("admin: {e}");
                    }
                })
            };
            let data = proxy::run(Arc::clone(&state));
            tokio::select! {
                r = data => { if let Err(e) = r { eprintln!("proxy: {e}"); } }
                _ = admin => {}
            }
        }
    }
}

fn usage() {
    eprintln!(
        "usage:\n  tiny-lb <listen> <backend> [backend...]\n  tiny-lb echo <port>\n  tiny-lb load <lb-addr> [conns] [seconds]"
    );
}

async fn echo(port: u16) -> std::io::Result<()> {
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
