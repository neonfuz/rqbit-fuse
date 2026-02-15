use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::debug;
use tracing::warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitState>>,
    failure_count: AtomicU32,
    failure_threshold: u32,
    timeout: Duration,
    opened_at: Arc<RwLock<Option<Instant>>>,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, timeout: Duration) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            failure_count: AtomicU32::new(0),
            failure_threshold,
            timeout,
            opened_at: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn can_execute(&self) -> bool {
        let state = *self.state.read().await;
        match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                let opened_at = *self.opened_at.read().await;
                if let Some(time) = opened_at {
                    if time.elapsed() >= self.timeout {
                        *self.state.write().await = CircuitState::HalfOpen;
                        debug!("Circuit breaker transitioning to half-open");
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    pub async fn record_success(&self) {
        self.failure_count.store(0, Ordering::SeqCst);
        let mut state = self.state.write().await;
        if *state != CircuitState::Closed {
            debug!("Circuit breaker closing");
            *state = CircuitState::Closed;
            *self.opened_at.write().await = None;
        }
    }

    pub async fn record_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
        if count >= self.failure_threshold {
            let mut state = self.state.write().await;
            if *state == CircuitState::Closed || *state == CircuitState::HalfOpen {
                warn!(
                    "Circuit breaker opened after {} consecutive failures",
                    count
                );
                *state = CircuitState::Open;
                *self.opened_at.write().await = Some(Instant::now());
            }
        }
    }

    pub async fn state(&self) -> CircuitState {
        *self.state.read().await
    }
}
