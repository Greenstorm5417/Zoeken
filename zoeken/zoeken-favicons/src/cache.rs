//! Favicon cache with two implementations: SQLite (default) and in-memory (tests).
//! Stores favicons keyed by (resolver, authority) with a known-missing marker.

use std::collections::HashMap;
use std::sync::Mutex;

/// A resolved favicon with raw bytes and MIME type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Favicon {
    pub data: Vec<u8>,
    pub mime: String,
}

impl Favicon {
    /// Construct a favicon from its bytes and MIME type.
    pub fn new(data: impl Into<Vec<u8>>, mime: impl Into<String>) -> Self {
        Self {
            data: data.into(),
            mime: mime.into(),
        }
    }
}

/// Result of a cache lookup: Hit, KnownMissing (cached negative), or Absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheLookup {
    Hit(Favicon),
    KnownMissing,
    Absent,
}

/// Cache interface for resolved favicons keyed by (resolver, authority).
pub trait FaviconCache: Send + Sync {
    fn get(&self, resolver: &str, authority: &str) -> CacheLookup;
    /// Set caches the favicon or records a known-missing marker (when None).
    fn set(&self, resolver: &str, authority: &str, favicon: Option<&Favicon>) -> bool;
}

pub const DEFAULT_BLOB_MAX_BYTES: usize = 20 * 1024;

pub struct SqliteFaviconCache {
    conn: Mutex<rusqlite::Connection>,
    blob_max_bytes: usize,
}

/// Sentinel hash used in `blob_map` to record a known-missing favicon.
const FALLBACK_HASH: &str = "FALLBACK_ICON";

impl SqliteFaviconCache {
    /// Open (creating if needed) a favicon cache at `path`.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, rusqlite::Error> {
        let conn = rusqlite::Connection::open(path.as_ref())?;
        Self::from_connection(conn)
    }

    /// Create an in-memory SQLite cache. Handy for tests (no file, no network).
    pub fn in_memory() -> Result<Self, rusqlite::Error> {
        let conn = rusqlite::Connection::open_in_memory()?;
        Self::from_connection(conn)
    }

    /// Build a cache over an existing connection, initializing the schema.
    pub fn from_connection(conn: rusqlite::Connection) -> Result<Self, rusqlite::Error> {
        Self::init_schema(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            blob_max_bytes: DEFAULT_BLOB_MAX_BYTES,
        })
    }

    /// Override the maximum BLOB size (bytes) that will be cached.
    pub fn with_blob_max_bytes(mut self, max: usize) -> Self {
        self.blob_max_bytes = max;
        self
    }

    fn init_schema(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS blobs (
                 sha256   TEXT PRIMARY KEY,
                 bytes_c  INTEGER,
                 mime     TEXT NOT NULL,
                 data     BLOB NOT NULL
             );
             CREATE TABLE IF NOT EXISTS blob_map (
                 m_time    INTEGER DEFAULT (strftime('%s','now')),
                 sha256    TEXT,
                 resolver  TEXT,
                 authority TEXT,
                 PRIMARY KEY (resolver, authority)
             );",
        )
    }

    fn sha256_hex(data: &[u8]) -> String {
        sha256::hex(data)
    }
}

impl FaviconCache for SqliteFaviconCache {
    fn get(&self, resolver: &str, authority: &str) -> CacheLookup {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return CacheLookup::Absent,
        };

        let sha: Option<String> = conn
            .query_row(
                "SELECT sha256 FROM blob_map WHERE resolver = ?1 AND authority = ?2",
                rusqlite::params![resolver, authority],
                |r| r.get(0),
            )
            .ok();

        let sha = match sha {
            Some(s) => s,
            None => return CacheLookup::Absent,
        };

        if sha == FALLBACK_HASH {
            return CacheLookup::KnownMissing;
        }

        let row: Option<(Vec<u8>, String)> = conn
            .query_row(
                "SELECT data, mime FROM blobs WHERE sha256 = ?1",
                rusqlite::params![sha],
                |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?)),
            )
            .ok();

        match row {
            Some((data, mime)) => CacheLookup::Hit(Favicon { data, mime }),
            // Dangling map entry: treat as absent.
            None => CacheLookup::Absent,
        }
    }

    fn set(&self, resolver: &str, authority: &str, favicon: Option<&Favicon>) -> bool {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return false,
        };

        let sha = match favicon {
            Some(f) => {
                if f.data.len() > self.blob_max_bytes {
                    return false;
                }
                let sha = Self::sha256_hex(&f.data);
                let _ = conn.execute(
                    "INSERT INTO blobs (sha256, bytes_c, mime, data) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT (sha256) DO NOTHING",
                    rusqlite::params![sha, f.data.len() as i64, f.mime, f.data],
                );
                sha
            }
            None => FALLBACK_HASH.to_string(),
        };

        conn.execute(
            "INSERT INTO blob_map (sha256, resolver, authority) VALUES (?1, ?2, ?3)
             ON CONFLICT (resolver, authority) DO UPDATE
                 SET sha256 = excluded.sha256, m_time = strftime('%s','now')",
            rusqlite::params![sha, resolver, authority],
        )
        .is_ok()
    }
}

/// In-memory favicon cache for tests.
#[derive(Default)]
pub struct InMemoryFaviconCache {
    map: Mutex<HashMap<(String, String), Option<Favicon>>>,
}

impl InMemoryFaviconCache {
    /// Create an empty in-memory cache.
    pub fn new() -> Self {
        Self::default()
    }
}

impl FaviconCache for InMemoryFaviconCache {
    fn get(&self, resolver: &str, authority: &str) -> CacheLookup {
        let map = match self.map.lock() {
            Ok(m) => m,
            Err(_) => return CacheLookup::Absent,
        };
        match map.get(&(resolver.to_string(), authority.to_string())) {
            Some(Some(f)) => CacheLookup::Hit(f.clone()),
            Some(None) => CacheLookup::KnownMissing,
            None => CacheLookup::Absent,
        }
    }

    fn set(&self, resolver: &str, authority: &str, favicon: Option<&Favicon>) -> bool {
        let mut map = match self.map.lock() {
            Ok(m) => m,
            Err(_) => return false,
        };
        map.insert(
            (resolver.to_string(), authority.to_string()),
            favicon.cloned(),
        );
        true
    }
}

mod sha256 {
    //! SHA-256 for content-addressing cached blobs.

    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    pub fn hex(data: &[u8]) -> String {
        let mut h: [u32; 8] = [
            0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
            0x5be0cd19,
        ];

        // Pre-processing (padding).
        let bit_len = (data.len() as u64).wrapping_mul(8);
        let mut msg = data.to_vec();
        msg.push(0x80);
        while msg.len() % 64 != 56 {
            msg.push(0);
        }
        msg.extend_from_slice(&bit_len.to_be_bytes());

        for chunk in msg.chunks_exact(64) {
            let mut w = [0u32; 64];
            for (i, word) in w.iter_mut().enumerate().take(16) {
                let b = i * 4;
                *word = u32::from_be_bytes([chunk[b], chunk[b + 1], chunk[b + 2], chunk[b + 3]]);
            }
            for i in 16..64 {
                let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                w[i] = w[i - 16]
                    .wrapping_add(s0)
                    .wrapping_add(w[i - 7])
                    .wrapping_add(s1);
            }

            let mut a = h[0];
            let mut b = h[1];
            let mut c = h[2];
            let mut d = h[3];
            let mut e = h[4];
            let mut f = h[5];
            let mut g = h[6];
            let mut hh = h[7];

            for i in 0..64 {
                let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let ch = (e & f) ^ ((!e) & g);
                let temp1 = hh
                    .wrapping_add(s1)
                    .wrapping_add(ch)
                    .wrapping_add(K[i])
                    .wrapping_add(w[i]);
                let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let maj = (a & b) ^ (a & c) ^ (b & c);
                let temp2 = s0.wrapping_add(maj);

                hh = g;
                g = f;
                f = e;
                e = d.wrapping_add(temp1);
                d = c;
                c = b;
                b = a;
                a = temp1.wrapping_add(temp2);
            }

            h[0] = h[0].wrapping_add(a);
            h[1] = h[1].wrapping_add(b);
            h[2] = h[2].wrapping_add(c);
            h[3] = h[3].wrapping_add(d);
            h[4] = h[4].wrapping_add(e);
            h[5] = h[5].wrapping_add(f);
            h[6] = h[6].wrapping_add(g);
            h[7] = h[7].wrapping_add(hh);
        }

        let mut out = String::with_capacity(64);
        for word in h {
            out.push_str(&format!("{word:08x}"));
        }
        out
    }

    #[cfg(test)]
    mod tests {
        use super::hex;

        #[test]
        fn known_vectors() {
            assert_eq!(
                hex(b""),
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            );
            assert_eq!(
                hex(b"abc"),
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
            );
        }
    }
}
