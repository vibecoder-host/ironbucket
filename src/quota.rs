use crate::models::{BucketQuota, BucketQuotaCache, BucketStats, Operation};
use chrono::{Datelike, Utc};
use serde_json;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use walkdir::WalkDir;

const DEFAULT_QUOTA_BYTES: u64 = 5 * 1024 * 1024 * 1024; // 5GB
const FLUSH_INTERVAL_SECS: u64 = 1;

pub struct QuotaManager {
    storage_path: PathBuf,
    quota_cache: Arc<RwLock<HashMap<String, BucketQuotaCache>>>,
    stats_cache: Arc<RwLock<HashMap<String, BucketStats>>>,
    flush_interval: Duration,
    enabled: bool,
}

impl QuotaManager {
    pub fn new(storage_path: PathBuf, enabled: bool) -> Self {
        let default_quota = env::var("BUCKET_QUOTA_BYTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_QUOTA_BYTES);

        let flush_interval_ms = env::var("QUOTA_FLUSH_INTERVAL_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(FLUSH_INTERVAL_SECS * 1000);

        QuotaManager {
            storage_path,
            quota_cache: Arc::new(RwLock::new(HashMap::new())),
            stats_cache: Arc::new(RwLock::new(HashMap::new())),
            flush_interval: Duration::from_millis(flush_interval_ms),
            enabled,
        }
    }

    // Check if quota and stats are enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    // Load quota from disk or generate from filesystem scan
    pub async fn load_or_generate_quota(&self, bucket: &str) -> io::Result<BucketQuota> {
        // If quota and stats are disabled, return unlimited quota without any I/O
        if !self.enabled {
            return Ok(BucketQuota {
                max_size_bytes: u64::MAX,
                current_usage_bytes: 0,
                object_count: 0,
                last_updated: Utc::now(),
            });
        }

        let mut cache = self.quota_cache.write().await;

        // Check if already in cache
        if let Some(cached) = cache.get(bucket) {
            return Ok(cached.quota.clone());
        }

        let bucket_path = self.storage_path.join(bucket);
        let quota_file = bucket_path.join(".quota");

        let quota = if quota_file.exists() {
            // Load from file
            match self.load_quota_from_file(&quota_file) {
                Ok(q) => q,
                Err(e) => {
                    warn!("Failed to load quota file for bucket {}, regenerating: {}", bucket, e);
                    self.generate_quota_from_fs(&bucket_path)?
                }
            }
        } else {
            // Generate from filesystem scan
            info!("No quota file found for bucket {}, generating from filesystem", bucket);
            self.generate_quota_from_fs(&bucket_path)?
        };

        // Add to cache
        cache.insert(
            bucket.to_string(),
            BucketQuotaCache {
                quota: quota.clone(),
                dirty: false,
                last_flush: Instant::now(),
            },
        );

        Ok(quota)
    }

    // Load quota from .quota file
    fn load_quota_from_file(&self, quota_file: &Path) -> io::Result<BucketQuota> {
        let content = fs::read_to_string(quota_file)?;
        serde_json::from_str(&content)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    // Generate quota by scanning filesystem
    fn generate_quota_from_fs(&self, bucket_path: &Path) -> io::Result<BucketQuota> {
        let mut total_size = 0u64;
        let mut object_count = 0u64;

        for entry in WalkDir::new(bucket_path)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                let file_name = entry.file_name().to_string_lossy();
                // Skip hidden files and metadata files
                if !file_name.starts_with('.') && !file_name.ends_with(".metadata") {
                    if let Ok(metadata) = entry.metadata() {
                        total_size += metadata.len();
                        object_count += 1;
                    }
                }
            }
        }

        let quota = BucketQuota {
            max_size_bytes: DEFAULT_QUOTA_BYTES,
            current_usage_bytes: total_size,
            object_count,
            last_updated: Utc::now(),
        };

        // Save to disk
        self.save_quota_to_file(&bucket_path.join(".quota"), &quota)?;

        Ok(quota)
    }

    // Save quota to .quota file
    fn save_quota_to_file(&self, quota_file: &Path, quota: &BucketQuota) -> io::Result<()> {
        let temp_file = quota_file.with_extension("tmp");
        let content = serde_json::to_string_pretty(quota)?;
        fs::write(&temp_file, content)?;
        fs::rename(temp_file, quota_file)?;
        Ok(())
    }

    // Check if adding new_size would exceed quota
    pub async fn check_quota(&self, bucket: &str, new_size: u64) -> io::Result<bool> {
        // If quota and stats are disabled, always allow
        if !self.enabled {
            return Ok(true);
        }

        let quota = self.load_or_generate_quota(bucket).await?;
        Ok(quota.current_usage_bytes + new_size <= quota.max_size_bytes)
    }

    // Update quota after adding an object
    pub async fn update_quota_add(&self, bucket: &str, size: u64) -> io::Result<()> {
        // If quota and stats are disabled, do nothing
        if !self.enabled {
            return Ok(());
        }
        let mut cache = self.quota_cache.write().await;

        // Ensure quota is loaded
        if !cache.contains_key(bucket) {
            drop(cache);
            self.load_or_generate_quota(bucket).await?;
            cache = self.quota_cache.write().await;
        }

        if let Some(cached) = cache.get_mut(bucket) {
            cached.quota.current_usage_bytes += size;
            cached.quota.object_count += 1;
            cached.quota.last_updated = Utc::now();
            cached.dirty = true;
        }

        Ok(())
    }

    // Update quota after removing an object
    pub async fn update_quota_remove(&self, bucket: &str, size: u64) -> io::Result<()> {
        // If quota and stats are disabled, do nothing
        if !self.enabled {
            return Ok(());
        }

        let mut cache = self.quota_cache.write().await;

        // Ensure quota is loaded
        if !cache.contains_key(bucket) {
            drop(cache);
            self.load_or_generate_quota(bucket).await?;
            cache = self.quota_cache.write().await;
        }

        if let Some(cached) = cache.get_mut(bucket) {
            cached.quota.current_usage_bytes = cached.quota.current_usage_bytes.saturating_sub(size);
            cached.quota.object_count = cached.quota.object_count.saturating_sub(1);
            cached.quota.last_updated = Utc::now();
            cached.dirty = true;
        }

        Ok(())
    }

    // Get quota information for a bucket
    pub async fn get_quota(&self, bucket: &str) -> io::Result<BucketQuota> {
        // If quota and stats are disabled, return a default quota with unlimited size
        if !self.enabled {
            return Ok(BucketQuota {
                max_size_bytes: u64::MAX,
                current_usage_bytes: 0,
                object_count: 0,
                last_updated: Utc::now(),
            });
        }

        self.load_or_generate_quota(bucket).await
    }

    // Load stats from disk for specific month
    fn load_stats_from_file(&self, stats_file: &Path) -> io::Result<BucketStats> {
        if stats_file.exists() {
            let content = fs::read_to_string(stats_file)?;
            serde_json::from_str(&content)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        } else {
            Ok(BucketStats::default())
        }
    }

    // Save stats to disk
    fn save_stats_to_file(&self, stats_file: &Path, stats: &BucketStats) -> io::Result<()> {
        // Create stats directory if it doesn't exist
        if let Some(parent) = stats_file.parent() {
            fs::create_dir_all(parent)?;
        }

        let temp_file = stats_file.with_extension("tmp");
        let content = serde_json::to_string_pretty(stats)?;
        fs::write(&temp_file, content)?;
        fs::rename(temp_file, stats_file)?;
        Ok(())
    }

    // Get current month stats file path
    fn get_current_stats_file(&self, bucket: &str) -> PathBuf {
        let now = Utc::now();
        let filename = format!("{:04}-{:02}.json", now.year(), now.month());
        self.storage_path
            .join(bucket)
            .join(".stats")
            .join(filename)
    }

    // Get stats file for specific month
    fn get_stats_file_for_month(&self, bucket: &str, year_month: &str) -> PathBuf {
        self.storage_path
            .join(bucket)
            .join(".stats")
            .join(format!("{}.json", year_month))
    }

    // Increment a stat counter
    pub async fn increment_stat(&self, bucket: &str, operation: Operation) -> io::Result<()> {
        // If quota and stats are disabled, do nothing
        if !self.enabled {
            return Ok(());
        }

        let stats_file = self.get_current_stats_file(bucket);
        let mut cache = self.stats_cache.write().await;

        let cache_key = format!("{}:{}", bucket, stats_file.display());
        let stats = cache.entry(cache_key.clone()).or_insert_with(|| {
            self.load_stats_from_file(&stats_file).unwrap_or_default()
        });

        match operation {
            Operation::Get => stats.get_count += 1,
            Operation::Put => stats.put_count += 1,
            Operation::Delete => stats.delete_count += 1,
            Operation::List => stats.list_count += 1,
            Operation::Head => stats.head_count += 1,
            Operation::Multipart => stats.multipart_count += 1,
        }

        Ok(())
    }

    // Get stats for a specific month
    pub async fn get_stats(&self, bucket: &str, year_month: Option<&str>) -> io::Result<BucketStats> {
        // If quota and stats are disabled, return empty stats
        if !self.enabled {
            return Ok(BucketStats::default());
        }

        let stats_file = if let Some(ym) = year_month {
            self.get_stats_file_for_month(bucket, ym)
        } else {
            self.get_current_stats_file(bucket)
        };

        self.load_stats_from_file(&stats_file)
    }

    // Flush all dirty quotas and stats to disk
    pub async fn flush_all(&self) -> io::Result<()> {
        // If quota and stats are disabled, do nothing
        if !self.enabled {
            return Ok(());
        }

        // Flush quotas
        {
            let mut cache = self.quota_cache.write().await;
            for (bucket, quota_cache) in cache.iter_mut() {
                if quota_cache.dirty {
                    let quota_file = self.storage_path.join(bucket).join(".quota");
                    match self.save_quota_to_file(&quota_file, &quota_cache.quota) {
                        Ok(_) => {
                            quota_cache.dirty = false;
                            quota_cache.last_flush = Instant::now();
                            debug!("Flushed quota for bucket: {}", bucket);
                        }
                        Err(e) => {
                            error!("Failed to flush quota for bucket {}: {}", bucket, e);
                        }
                    }
                }
            }
        }

        // Flush stats
        {
            let cache = self.stats_cache.read().await;
            for (cache_key, stats) in cache.iter() {
                // Extract bucket name and stats file from cache key
                if let Some(colon_pos) = cache_key.find(':') {
                    let bucket = &cache_key[..colon_pos];
                    let stats_file = self.get_current_stats_file(bucket);
                    match self.save_stats_to_file(&stats_file, stats) {
                        Ok(_) => {
                            debug!("Flushed stats for bucket: {}", bucket);
                        }
                        Err(e) => {
                            error!("Failed to flush stats for bucket {}: {}", bucket, e);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    // Background task to periodically flush quotas and stats
    pub async fn start_flush_task(self: Arc<Self>) {
        // If quota and stats are disabled, don't run the flush task at all
        if !self.enabled {
            debug!("Quota and stats flush task skipped - quota and stats are disabled");
            return;
        }

        let mut interval = interval(self.flush_interval);

        loop {
            interval.tick().await;
            if let Err(e) = self.flush_all().await {
                error!("Error during periodic flush: {}", e);
            }
        }
    }
}