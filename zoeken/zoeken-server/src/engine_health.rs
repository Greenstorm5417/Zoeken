//! Persisted aggregate engine health and circuit transitions.

use std::sync::Arc;
use std::time::{Duration, Instant};

use zoeken_engine_core::{EngineError, EngineResults};
use zoeken_storage::{EngineHealthSnapshot, EngineHealthUpdate, Storage};

pub(crate) struct PendingHealth {
    storage: Option<Arc<dyn Storage>>,
    engine: String,
    previous: Option<EngineHealthSnapshot>,
    started: Instant,
    complete: bool,
}

impl PendingHealth {
    pub(crate) fn new(
        storage: Option<Arc<dyn Storage>>,
        engine: String,
        previous: Option<EngineHealthSnapshot>,
    ) -> Self {
        Self {
            storage,
            engine,
            previous,
            started: Instant::now(),
            complete: false,
        }
    }

    pub(crate) fn complete(&mut self) {
        self.complete = true;
    }
}

impl Drop for PendingHealth {
    fn drop(&mut self) {
        if self.complete {
            return;
        }
        let Some(storage) = self.storage.clone() else {
            return;
        };
        let engine = self.engine.clone();
        let previous = self.previous.clone();
        let duration = self.started.elapsed();
        tokio::spawn(async move {
            record_health(
                Some(storage.as_ref()),
                &engine,
                duration,
                &Err(EngineError::Timeout),
                previous.as_ref(),
            )
            .await;
        });
    }
}

fn unix_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as i64)
}

pub(crate) fn circuit_is_open(snapshot: Option<&EngineHealthSnapshot>) -> bool {
    snapshot.is_some_and(|health| {
        health.circuit_status == "open"
            && health
                .cooldown_until_ms
                .is_some_and(|until| until > unix_ms())
    })
}

fn error_category(error: &EngineError) -> &'static str {
    match error {
        EngineError::AccessDenied(_) | EngineError::CloudflareAccessDenied(_) => "access_denied",
        EngineError::Captcha(_)
        | EngineError::CloudflareCaptcha(_)
        | EngineError::RecaptchaCaptcha(_) => "captcha",
        EngineError::TooManyRequests(_) => "throttle",
        EngineError::Parse(_) => "malformed",
        EngineError::Timeout => "timeout",
        EngineError::QueueExpired => "queue_expired",
        EngineError::Unexpected(_) => "transport",
    }
}

pub(crate) fn cooldown_for(
    engine: &str,
    error: &EngineError,
    previous: Option<&EngineHealthSnapshot>,
) -> Option<Duration> {
    let category = error_category(error);
    let base = match error {
        EngineError::Captcha(_) if engine == "duckduckgo" => {
            let jitter = (unix_ms().unsigned_abs() % 601) + 300;
            Duration::from_secs(jitter)
        }
        EngineError::Captcha(_)
        | EngineError::CloudflareCaptcha(_)
        | EngineError::RecaptchaCaptcha(_)
        | EngineError::AccessDenied(_)
        | EngineError::CloudflareAccessDenied(_) => Duration::from_secs(15 * 60),
        EngineError::TooManyRequests(_) => Duration::from_secs(5 * 60),
        EngineError::Parse(_) if previous.is_some_and(|health| health.errors >= 2) => {
            Duration::from_secs(60)
        }
        EngineError::Timeout if previous.is_some_and(|health| health.timeouts >= 2) => {
            Duration::from_secs(60)
        }
        EngineError::Unexpected(_) if previous.is_some_and(|health| health.errors >= 2) => {
            Duration::from_secs(30)
        }
        EngineError::QueueExpired => return None,
        _ => return None,
    };
    let recurrent = previous.is_some_and(|health| {
        health.last_error_category.as_deref() == Some(category)
            && matches!(health.circuit_status.as_str(), "open" | "half_open")
    });
    Some(if recurrent {
        base.saturating_mul(2)
            .min(Duration::from_secs(24 * 60 * 60))
    } else {
        base
    })
}

pub(crate) async fn record_health(
    storage: Option<&dyn Storage>,
    engine: &str,
    duration: Duration,
    result: &Result<EngineResults, EngineError>,
    previous: Option<&EngineHealthSnapshot>,
) {
    let Some(storage) = storage else {
        return;
    };
    let now = unix_ms();
    let (success, timed_out, category, circuit_status, cooldown_until_ms) = match result {
        Ok(_) => {
            let status = match previous.map(|health| health.circuit_status.as_str()) {
                Some("open") => "half_open",
                Some("half_open") => "closed",
                _ => "closed",
            };
            (true, false, None, status, None)
        }
        Err(error) => {
            let cooldown = cooldown_for(engine, error, previous);
            (
                false,
                matches!(error, EngineError::Timeout),
                Some(error_category(error).to_string()),
                if cooldown.is_some() { "open" } else { "closed" },
                cooldown.map(|value| now.saturating_add(value.as_millis() as i64)),
            )
        }
    };
    let transition = circuit_status.to_string();
    let update = EngineHealthUpdate {
        engine: engine.to_string(),
        bucket: now / 3_600_000,
        latency_ms: duration.as_millis().min(u128::from(u64::MAX)) as u64,
        success,
        timed_out,
        error_category: category,
        circuit_status: transition.clone(),
        cooldown_until_ms,
    };
    if storage.record_engine_health(&update).await.is_err() {
        metrics::counter!("storage_operations_total", "operation" => "engine_health", "outcome" => "error")
            .increment(1);
    } else if previous.map(|health| health.circuit_status.as_str()) != Some(transition.as_str()) {
        metrics::counter!("engine_circuit_total", "transition" => transition).increment(1);
    }
}
