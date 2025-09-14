use crate::{config::Config, error::Result};
use deadpool_redis::{Config as RedisConfig, Pool, Runtime};
use redis::AsyncCommands;
use std::time::Duration;
use tracing::{debug, error, warn};

pub struct CacheManager {
    pool: Pool,
    enabled: bool,
}

impl CacheManager {
    pub async fn new(config: &Config) -> Result<Self> {
        let redis_config = RedisConfig::from_url(config.redis.url.clone());
        let pool = redis_config
            .create_pool(Some(Runtime::Tokio1))
            .map_err(|e| anyhow::anyhow!("Failed to create Redis pool: {}", e))?;

        // Test connection
        let enabled = match pool.get().await {
            Ok(mut conn) => {
                match redis::cmd("PING").query_async::<_, String>(&mut conn).await {
                    Ok(_) => {
                        debug!("Redis cache connected successfully");
                        true
                    }
                    Err(e) => {
                        warn!("Redis cache not available: {}", e);
                        false
                    }
                }
            }
            Err(e) => {
                warn!("Redis cache not available: {}", e);
                false
            }
        };

        Ok(Self { pool, enabled })
    }

    pub async fn get(&self, key: &str) -> Option<String> {
        if !self.enabled {
            return None;
        }

        match self.pool.get().await {
            Ok(mut conn) => {
                match conn.get::<_, String>(key).await {
                    Ok(value) => {
                        debug!("Cache hit for key: {}", key);
                        Some(value)
                    }
                    Err(_) => {
                        debug!("Cache miss for key: {}", key);
                        None
                    }
                }
            }
            Err(e) => {
                error!("Failed to get Redis connection: {}", e);
                None
            }
        }
    }

    pub async fn get_bytes(&self, key: &str) -> Option<Vec<u8>> {
        if !self.enabled {
            return None;
        }

        match self.pool.get().await {
            Ok(mut conn) => {
                match conn.get::<_, Vec<u8>>(key).await {
                    Ok(value) => {
                        debug!("Cache hit for key: {}", key);
                        Some(value)
                    }
                    Err(_) => {
                        debug!("Cache miss for key: {}", key);
                        None
                    }
                }
            }
            Err(e) => {
                error!("Failed to get Redis connection: {}", e);
                None
            }
        }
    }

    pub async fn set(&self, key: &str, value: String, ttl_seconds: u64) {
        if !self.enabled {
            return;
        }

        if let Ok(mut conn) = self.pool.get().await {
            let _ = conn
                .set_ex::<_, _, String>(key, value, ttl_seconds)
                .await
                .map_err(|e| {
                    error!("Failed to set cache key {}: {}", key, e);
                });
            debug!("Cached key: {} for {} seconds", key, ttl_seconds);
        }
    }

    pub async fn set_bytes(&self, key: &str, value: Vec<u8>, ttl_seconds: u64) {
        if !self.enabled {
            return;
        }

        if let Ok(mut conn) = self.pool.get().await {
            let _ = conn
                .set_ex::<_, _, String>(key, value, ttl_seconds)
                .await
                .map_err(|e| {
                    error!("Failed to set cache key {}: {}", key, e);
                });
            debug!("Cached key: {} for {} seconds", key, ttl_seconds);
        }
    }

    pub async fn delete(&self, key: &str) {
        if !self.enabled {
            return;
        }

        if let Ok(mut conn) = self.pool.get().await {
            let _ = conn.del::<_, u32>(key).await.map_err(|e| {
                error!("Failed to delete cache key {}: {}", key, e);
            });
            debug!("Deleted cache key: {}", key);
        }
    }

    pub async fn invalidate_bucket(&self, bucket: &str) {
        if !self.enabled {
            return;
        }

        // Invalidate all keys related to this bucket
        let pattern = format!("*{}*", bucket);
        if let Ok(mut conn) = self.pool.get().await {
            if let Ok(keys) = conn.keys::<_, Vec<String>>(&pattern).await {
                let count = keys.len();
                for key in keys {
                    let _ = conn.del::<_, u32>(&key).await;
                }
                debug!("Invalidated {} cache entries for bucket: {}", count, bucket);
            }
        }
    }

    pub async fn invalidate_object(&self, bucket: &str, key: &str) {
        if !self.enabled {
            return;
        }

        // Invalidate specific object and related list caches
        let object_key = format!("obj:{}:{}", bucket, key);
        let list_pattern = format!("list:{}:*", bucket);

        if let Ok(mut conn) = self.pool.get().await {
            // Delete object cache
            let _ = conn.del::<_, u32>(&object_key).await;

            // Delete list caches for this bucket
            if let Ok(keys) = conn.keys::<_, Vec<String>>(&list_pattern).await {
                for key in keys {
                    let _ = conn.del::<_, u32>(&key).await;
                }
            }
            debug!("Invalidated cache for object: {}/{}", bucket, key);
        }
    }

    pub async fn clear_all(&self) {
        if !self.enabled {
            return;
        }

        if let Ok(mut conn) = self.pool.get().await {
            let _ = redis::cmd("FLUSHDB")
                .query_async::<_, String>(&mut conn)
                .await
                .map_err(|e| {
                    error!("Failed to clear cache: {}", e);
                });
            debug!("Cleared all cache entries");
        }
    }
}