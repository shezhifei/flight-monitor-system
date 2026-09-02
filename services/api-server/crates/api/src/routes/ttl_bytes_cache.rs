//! Process-local TTL cache of pre-serialized JSON response bodies.

use std::sync::Arc;
use std::time::{Duration, Instant};

use actix_web::{http::header, web, HttpResponse};
use arc_swap::ArcSwapOption;

struct CacheEntry {
    cached_at: Instant,
    body: web::Bytes,
}

pub struct TtlBytesCache {
    slot: ArcSwapOption<CacheEntry>,
    ttl: Duration,
}

impl TtlBytesCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            slot: ArcSwapOption::empty(),
            ttl,
        }
    }

    pub fn get(&self) -> Option<web::Bytes> {
        let entry = self.slot.load_full()?;
        if entry.cached_at.elapsed() <= self.ttl {
            Some(entry.body.clone())
        } else {
            None
        }
    }

    pub fn store(&self, body: web::Bytes) {
        self.slot.store(Some(Arc::new(CacheEntry {
            cached_at: Instant::now(),
            body,
        })));
    }

    #[allow(dead_code)]
    pub fn invalidate(&self) {
        self.slot.store(None);
    }
}

pub fn json_bytes_response(body: web::Bytes) -> HttpResponse {
    HttpResponse::Ok()
        .insert_header((header::CONTENT_TYPE, "application/json"))
        .body(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;
    use std::thread;

    #[test]
    fn ttl_bytes_cache_serves_concurrent_reads_and_invalidation() {
        let cache = Arc::new(TtlBytesCache::new(Duration::from_secs(1)));
        cache.store(web::Bytes::from_static(b"cached"));

        let barrier = Arc::new(Barrier::new(9));
        let mut readers = Vec::new();
        for _ in 0..8 {
            let cache = Arc::clone(&cache);
            let barrier = Arc::clone(&barrier);
            readers.push(thread::spawn(move || {
                barrier.wait();
                for _ in 0..1000 {
                    let body = cache.get();
                    assert!(match body.as_deref() {
                        None => true,
                        Some(bytes) => bytes == b"cached" || bytes == b"refreshed",
                    });
                }
            }));
        }

        barrier.wait();
        cache.invalidate();
        assert!(cache.get().is_none());
        cache.store(web::Bytes::from_static(b"refreshed"));

        for reader in readers {
            reader.join().expect("cache reader thread should finish");
        }

        assert_eq!(cache.get().as_deref(), Some(&b"refreshed"[..]));
        cache.invalidate();
    }
}
