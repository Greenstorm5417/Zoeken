//! Persisted aggregate engine health and circuit transitions.

use std::sync::Arc;
use std::time::{Duration, Instant};

use zoeken_engine_core::{EngineError, EngineResults, ErrorCategory};
use zoeken_search::SuspensionPolicy;
use zoeken_storage::{EngineHealthSnapshot, EngineHealthUpdate, Storage};

pub(crate) struct PendingHealth {
    storage: Option<Arc<dyn Storage>>,
    engine: String,
    previous: Option<EngineHealthSnapshot>,
    policy: SuspensionPolicy,
    started: Instant,
    complete: bool,
}

impl PendingHealth {
    pub(crate) fn new(
        storage: Option<Arc<dyn Storage>>,
        engine: String,
        previous: Option<EngineHealthSnapshot>,
        policy: SuspensionPolicy,
    ) -> Self {
        Self {
            storage,
            engine,
            previous,
            policy,
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
        let policy = self.policy;
        let duration = self.started.elapsed();
        tokio::spawn(async move {
            record_health(
                Some(storage.as_ref()),
                &engine,
                duration,
                &Err(EngineError::Timeout),
                previous.as_ref(),
                &policy,
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

fn crosses_threshold(previous_count: u64, threshold: u32) -> bool {
    threshold != 0 && previous_count.saturating_add(1) >= u64::from(threshold)
}

pub(crate) fn cooldown_for(
    engine: &str,
    error: &EngineError,
    previous: Option<&EngineHealthSnapshot>,
    policy: &SuspensionPolicy,
) -> Option<Duration> {
    let category = ErrorCategory::from(error).as_str();
    let base = if let Some(explicit) = policy.explicit_duration(engine, error) {
        if explicit.is_zero() {
            return None;
        }
        explicit
    } else {
        match error {
            EngineError::Parse(_)
                if crosses_threshold(
                    previous.map_or(0, |health| health.errors),
                    policy.config.threshold,
                ) =>
            {
                policy.config.base
            }
            EngineError::Timeout
                if crosses_threshold(
                    previous.map_or(0, |health| health.timeouts),
                    policy.config.threshold,
                ) =>
            {
                policy.config.base
            }
            EngineError::Unexpected(_)
                if crosses_threshold(
                    previous.map_or(0, |health| health.errors),
                    policy.config.threshold,
                ) =>
            {
                policy.config.base
            }
            EngineError::QueueExpired => return None,
            _ => return None,
        }
    };
    if base.is_zero() {
        return None;
    }
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
    policy: &SuspensionPolicy,
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
            let cooldown = cooldown_for(engine, error, previous, policy);
            (
                false,
                matches!(error, EngineError::Timeout),
                Some(ErrorCategory::from(error).as_str().to_string()),
                if cooldown.is_some() { "open" } else { "closed" },
                cooldown.map(|value| now.saturating_add(value.as_millis() as i64)),
            )
        }
    };
    let transition = circuit_status.to_string();
    let category_label = category.clone();
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
        metrics::counter!("engine_circuit_total", "transition" => transition.clone()).increment(1);
        if transition == "open" {
            let cooldown_ms = cooldown_until_ms.map(|until| until.saturating_sub(now));
            tracing::warn!(
                engine,
                category = category_label.as_deref().unwrap_or("unknown"),
                cooldown_ms,
                "engine circuit opened (suspended)"
            );
        } else {
            tracing::info!(
                engine,
                circuit = transition.as_str(),
                "engine circuit transition"
            );
        }
    }
}
