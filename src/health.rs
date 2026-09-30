use crate::state::{
    Backend, State, FAIL_THRESHOLD, HEALTH_INTERVAL_MS, HEALTH_TIMEOUT_MS, OK_THRESHOLD,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;

pub fn spawn(state: Arc<State>) {
    tokio::spawn(async move {
        loop {
            for b in &state.backends {
                if b.disabled.load(Ordering::Relaxed) {
                    continue;
                }
                probe(b).await;
            }
            tokio::time::sleep(Duration::from_millis(HEALTH_INTERVAL_MS)).await;
        }
    });
}

async fn probe(b: &Backend) {
    let ok = timeout(
        Duration::from_millis(HEALTH_TIMEOUT_MS),
        TcpStream::connect(b.addr),
    )
    .await
    .ok()
    .and_then(Result::ok)
    .is_some();

    if ok {
        b.consec_fail.store(0, Ordering::Relaxed);
        let n = b.consec_ok.fetch_add(1, Ordering::Relaxed) + 1;
        if n >= OK_THRESHOLD as usize {
            b.healthy.store(true, Ordering::Relaxed);
        }
    } else {
        b.consec_ok.store(0, Ordering::Relaxed);
        let n = b.consec_fail.fetch_add(1, Ordering::Relaxed) + 1;
        if n >= FAIL_THRESHOLD as usize {
            b.healthy.store(false, Ordering::Relaxed);
        }
    }
}
