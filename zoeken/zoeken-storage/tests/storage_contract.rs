use std::time::Duration;

use zoeken_storage::{
    FaviconData, FaviconLookup, FaviconPolicy, OriginPolicy, PermitDecision, PostgresStorage,
    SqliteStorage, Storage,
};

async fn run_contract(storage: &dyn Storage, namespace: &str) {
    storage.healthcheck().await.unwrap();
    let origin = format!("https://{namespace}.example");
    let policy = OriginPolicy {
        requests_per_second: 10.0,
        burst: 2,
        max_concurrent: 1,
        lease_duration: Duration::from_secs(2),
    };
    let first = storage.acquire_origin(&origin, &policy).await.unwrap();
    assert_eq!(first.decision, PermitDecision::Granted);
    assert_eq!(
        storage
            .acquire_origin(&origin, &policy)
            .await
            .unwrap()
            .decision,
        PermitDecision::ConcurrencyLimited
    );
    storage
        .release_origin(first.lease.as_ref().unwrap())
        .await
        .unwrap();

    let authority = format!("{namespace}.example");
    let favicon = FaviconData {
        data: vec![1, 2, 3, 4],
        mime: "image/png".into(),
    };
    let favicon_policy = FaviconPolicy {
        positive_ttl: Duration::from_secs(60),
        negative_ttl: Duration::from_secs(10),
        max_blob_bytes: 1024,
        max_total_bytes: 4096,
    };
    assert!(
        storage
            .favicon_put("contract", &authority, Some(&favicon), &favicon_policy)
            .await
            .unwrap()
    );
    assert_eq!(
        storage.favicon_get("contract", &authority).await.unwrap(),
        FaviconLookup::Hit(favicon)
    );
}

#[tokio::test]
async fn sqlite_implements_storage_contract() {
    let storage = SqliteStorage::in_memory().await.unwrap();
    run_contract(&storage, "sqlite-contract").await;
}

#[tokio::test]
async fn sqlite_migrations_are_idempotent_and_concurrent_safe() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("contract.sqlite3");
    let (first, second) = tokio::join!(
        SqliteStorage::connect(&path, Duration::from_secs(5), 2),
        SqliteStorage::connect(&path, Duration::from_secs(5), 2),
    );
    first.unwrap().healthcheck().await.unwrap();
    second.unwrap().healthcheck().await.unwrap();
}

#[tokio::test]
async fn postgres_implements_storage_contract_when_configured() {
    let Some(url) = std::env::var("TEST_POSTGRES_URL")
        .ok()
        .filter(|url| !url.is_empty())
    else {
        eprintln!("TEST_POSTGRES_URL is unset; skipping PostgreSQL storage contract");
        return;
    };
    let storage = PostgresStorage::connect(&url, 4, Duration::from_secs(5))
        .await
        .unwrap();
    run_contract(
        &storage,
        &format!("postgres-contract-{}", std::process::id()),
    )
    .await;
}
