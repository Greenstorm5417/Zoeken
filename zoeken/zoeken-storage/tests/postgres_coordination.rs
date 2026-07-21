use std::time::Duration;

use zoeken_storage::{EngineHealthUpdate, OriginPolicy, PermitDecision, PostgresStorage, Storage};

fn test_url() -> Option<String> {
    std::env::var("TEST_POSTGRES_URL")
        .ok()
        .filter(|url| !url.is_empty())
}

#[tokio::test]
async fn two_instances_share_leases_retry_after_and_health() {
    let Some(url) = test_url() else {
        eprintln!("TEST_POSTGRES_URL is unset; skipping PostgreSQL coordination test");
        return;
    };
    let first = PostgresStorage::connect(&url, 4, Duration::from_secs(5))
        .await
        .unwrap();
    let second = PostgresStorage::connect(&url, 4, Duration::from_secs(5))
        .await
        .unwrap();
    let suffix = std::process::id();
    let origin = format!("https://contract-{suffix}.example");
    let engine = format!("contract-{suffix}");
    let policy = OriginPolicy {
        requests_per_second: 100.0,
        burst: 10,
        max_concurrent: 1,
        lease_duration: Duration::from_millis(75),
    };

    let lease = first.acquire_origin(&origin, &policy).await.unwrap();
    assert_eq!(lease.decision, PermitDecision::Granted);
    assert_eq!(
        second
            .acquire_origin(&origin, &policy)
            .await
            .unwrap()
            .decision,
        PermitDecision::ConcurrencyLimited
    );

    // Simulate a crashed replica by not releasing. The other instance may
    // recover the permit after the persisted lease expires.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let recovered = second.acquire_origin(&origin, &policy).await.unwrap();
    assert_eq!(recovered.decision, PermitDecision::Granted);
    second
        .release_origin(recovered.lease.as_ref().unwrap())
        .await
        .unwrap();

    first
        .defer_origin(&origin, Duration::from_secs(10))
        .await
        .unwrap();
    assert_eq!(
        second
            .acquire_origin(&origin, &policy)
            .await
            .unwrap()
            .decision,
        PermitDecision::RateLimited
    );

    first
        .record_engine_health(&EngineHealthUpdate {
            engine: engine.clone(),
            bucket: 42,
            latency_ms: 5,
            success: false,
            timed_out: false,
            error_category: Some("captcha".into()),
            circuit_status: "open".into(),
            cooldown_until_ms: Some(i64::MAX / 2),
        })
        .await
        .unwrap();
    let health = second.latest_engine_health(&engine).await.unwrap().unwrap();
    assert_eq!(health.circuit_status, "open");
    assert_eq!(health.last_error_category.as_deref(), Some("captcha"));
}
