//! In-memory favicon cache used by unit tests and explicitly non-persistent
//! application states. Production persistence lives in `zoeken-storage`.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Favicon {
    pub data: Vec<u8>,
    pub mime: String,
}

impl Favicon {
    pub fn new(data: impl Into<Vec<u8>>, mime: impl Into<String>) -> Self {
        Self {
            data: data.into(),
            mime: mime.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheLookup {
    Hit(Favicon),
    KnownMissing,
    Absent,
}

#[async_trait]
pub trait FaviconCache: Send + Sync {
    async fn get(&self, resolver: &str, authority: &str) -> CacheLookup;
    async fn set(&self, resolver: &str, authority: &str, favicon: Option<&Favicon>) -> bool;
}

#[derive(Default)]
pub struct InMemoryFaviconCache {
    map: Mutex<HashMap<(String, String), Option<Favicon>>>,
}

impl InMemoryFaviconCache {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl FaviconCache for InMemoryFaviconCache {
    async fn get(&self, resolver: &str, authority: &str) -> CacheLookup {
        let map = match self.map.lock() {
            Ok(map) => map,
            Err(_) => return CacheLookup::Absent,
        };
        match map.get(&(resolver.to_string(), authority.to_string())) {
            Some(Some(favicon)) => CacheLookup::Hit(favicon.clone()),
            Some(None) => CacheLookup::KnownMissing,
            None => CacheLookup::Absent,
        }
    }

    async fn set(&self, resolver: &str, authority: &str, favicon: Option<&Favicon>) -> bool {
        let mut map = match self.map.lock() {
            Ok(map) => map,
            Err(_) => return false,
        };
        map.insert(
            (resolver.to_string(), authority.to_string()),
            favicon.cloned(),
        );
        true
    }
}
