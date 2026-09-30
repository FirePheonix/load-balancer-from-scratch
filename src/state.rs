use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

pub const CONNECT_TIMEOUT_MS: u64 = 1000;
pub const IDLE_TIMEOUT_MS: u64 = 30_000;
pub const HEALTH_INTERVAL_MS: u64 = 2000;
pub const HEALTH_TIMEOUT_MS: u64 = 500;
pub const FAIL_THRESHOLD: u32 = 3;
pub const OK_THRESHOLD: u32 = 2;
pub const MAX_CONNS: u64 = 10_000;

pub struct Backend {
    pub addr: SocketAddr,
    pub healthy: AtomicBool,
    pub draining: AtomicBool,
    pub disabled: AtomicBool,
    pub active: AtomicU64,
    pub total: AtomicU64,
    pub connect_fail: AtomicU64,
    pub consec_fail: AtomicUsize,
    pub consec_ok: AtomicUsize,
}

impl Backend {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            healthy: AtomicBool::new(true),
            draining: AtomicBool::new(false),
            disabled: AtomicBool::new(false),
            active: AtomicU64::new(0),
            total: AtomicU64::new(0),
            connect_fail: AtomicU64::new(0),
            consec_fail: AtomicUsize::new(0),
            consec_ok: AtomicUsize::new(0),
        }
    }

    pub fn can_accept(&self) -> bool {
        self.healthy.load(Ordering::Relaxed)
            && !self.draining.load(Ordering::Relaxed)
            && !self.disabled.load(Ordering::Relaxed)
    }
}

pub struct State {
    pub listen: SocketAddr,
    pub admin: SocketAddr,
    pub backends: Vec<Backend>,
    pub rr: AtomicUsize,
    pub active: AtomicU64,
    pub opened: AtomicU64,
    pub failed: AtomicU64,
    pub rejected: AtomicU64,
    pub bytes: AtomicU64,
}

impl State {
    pub fn new(listen: SocketAddr, backends: Vec<SocketAddr>) -> Self {
        Self {
            listen,
            admin: "127.0.0.1:19090".parse().unwrap(),
            backends: backends.into_iter().map(Backend::new).collect(),
            rr: AtomicUsize::new(0),
            active: AtomicU64::new(0),
            opened: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    pub fn pick(&self) -> Option<usize> {
        let n = self.backends.len();
        if n == 0 {
            return None;
        }
        let start = self.rr.fetch_add(1, Ordering::Relaxed);
        for off in 0..n {
            let i = (start + off) % n;
            if self.backends[i].can_accept() {
                return Some(i);
            }
        }
        None
    }

    pub fn by_addr(&self, addr: SocketAddr) -> Option<&Backend> {
        self.backends.iter().find(|b| b.addr == addr)
    }
}
