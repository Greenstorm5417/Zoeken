//! Generic bounded in-process cache with per-key async singleflight.
//!
//! Callers supply a weight function so byte budgets (response bytes) and
//! entry-count caps both fit the same eviction loop.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Entry<V> {
    at: Instant,
    ttl: Duration,
    weight: usize,
    value: V,
}

pub struct FlightCache<K, V> {
    entries: Mutex<HashMap<K, Entry<V>>>,
    flights: Mutex<HashMap<K, Arc<tokio::sync::Mutex<()>>>>,
    total_weight: Mutex<usize>,
    max_weight: usize,
    weight_fn: Box<dyn Fn(&V) -> usize + Send + Sync>,
}

impl<K, V> FlightCache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    /// `max_weight` bounds the sum of `weight_fn` over all live entries
    /// (e.g. total response bytes, or entry count when `weight_fn` is `|_| 1`).
    pub fn new(max_weight: usize, weight_fn: impl Fn(&V) -> usize + Send + Sync + 'static) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            flights: Mutex::new(HashMap::new()),
            total_weight: Mutex::new(0),
            max_weight: max_weight.max(1),
            weight_fn: Box::new(weight_fn),
        }
    }

    /// Fetch a live (non-expired) entry, evicting it first if it has expired.
    pub fn get(&self, key: &K) -> Option<V> {
        let mut entries = self.entries.lock().ok()?;
        let entry = entries.get(key)?;
        if entry.at.elapsed() < entry.ttl {
            return Some(entry.value.clone());
        }
        let expired = entries.remove(key)?;
        if let Ok(mut total) = self.total_weight.lock() {
            *total = total.saturating_sub(expired.weight);
        }
        None
    }

    /// Insert or replace an entry with the given TTL, sweeping expired
    /// entries and evicting oldest-first until back under `max_weight`.
    /// A value heavier than `max_weight` on its own is silently dropped.
    pub fn put(&self, key: K, value: V, ttl: Duration) {
        let weight = (self.weight_fn)(&value);
        if weight > self.max_weight {
            return;
        }
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        let Ok(mut total) = self.total_weight.lock() else {
            return;
        };
        entries.retain(|_, entry| {
            let keep = entry.at.elapsed() < entry.ttl;
            if !keep {
                *total = total.saturating_sub(entry.weight);
            }
            keep
        });
        while total.saturating_add(weight) > self.max_weight {
            let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, entry)| entry.at)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(removed) = entries.remove(&oldest) {
                *total = total.saturating_sub(removed.weight);
            }
        }
        if let Some(previous) = entries.insert(
            key,
            Entry {
                at: Instant::now(),
                ttl,
                weight,
                value,
            },
        ) {
            *total = total.saturating_sub(previous.weight);
        }
        *total = total.saturating_add(weight);
    }

    /// Acquire (creating if absent) the per-key lock used to dedupe
    /// concurrent identical in-flight requests. Caller must `.lock().await`
    /// it and call [`Self::finish_flight`] when done.
    pub fn flight(&self, key: &K) -> Option<Arc<tokio::sync::Mutex<()>>> {
        let mut flights = self.flights.lock().ok()?;
        Some(
            flights
                .entry(key.clone())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone(),
        )
    }

    pub fn finish_flight(&self, key: &K) {
        if let Ok(mut flights) = self.flights.lock() {
            flights.remove(key);
        }
    }

    #[cfg(test)]
    fn total_weight(&self) -> usize {
        self.total_weight.lock().map(|w| *w).unwrap_or(0)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.lock().map(|e| e.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_put_round_trip_and_ttl_expiry() {
        let cache: FlightCache<String, String> = FlightCache::new(1024, |v: &String| v.len());
        assert!(cache.get(&"a".to_string()).is_none());
        cache.put(
            "a".to_string(),
            "hello".to_string(),
            Duration::from_millis(20),
        );
        assert_eq!(cache.get(&"a".to_string()), Some("hello".to_string()));
        std::thread::sleep(Duration::from_millis(30));
        assert!(cache.get(&"a".to_string()).is_none());
    }

    #[test]
    fn evicts_oldest_first_when_over_weight_budget() {
        let cache: FlightCache<String, String> = FlightCache::new(3, |v: &String| v.len());
        cache.put("a".to_string(), "x".to_string(), Duration::from_secs(60));
        cache.put("b".to_string(), "y".to_string(), Duration::from_secs(60));
        cache.put("c".to_string(), "z".to_string(), Duration::from_secs(60));
        // "a" should be evicted to make room.
        cache.put("d".to_string(), "w".to_string(), Duration::from_secs(60));
        assert!(cache.get(&"a".to_string()).is_none());
        assert!(cache.get(&"d".to_string()).is_some());
        assert!(cache.total_weight() <= 3);
    }

    #[test]
    fn oversized_entry_is_dropped_not_stored() {
        let cache: FlightCache<String, String> = FlightCache::new(2, |v: &String| v.len());
        cache.put(
            "a".to_string(),
            "too big".to_string(),
            Duration::from_secs(60),
        );
        assert!(cache.get(&"a".to_string()).is_none());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn count_based_weight_caps_entry_count() {
        let cache: FlightCache<u32, u32> = FlightCache::new(2, |_: &u32| 1);
        cache.put(1, 1, Duration::from_secs(60));
        cache.put(2, 2, Duration::from_secs(60));
        cache.put(3, 3, Duration::from_secs(60));
        assert!(cache.len() <= 2);
    }

    #[tokio::test]
    async fn flight_returns_the_same_lock_for_the_same_key() {
        let cache: FlightCache<String, u32> = FlightCache::new(10, |_: &u32| 1);
        let first = cache.flight(&"k".to_string()).unwrap();
        let second = cache.flight(&"k".to_string()).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        cache.finish_flight(&"k".to_string());
        let third = cache.flight(&"k".to_string()).unwrap();
        assert!(!Arc::ptr_eq(&first, &third));
    }
}
