# IronBucket Quota and Stats Implementation TODO

## Overview

Implement bucket quotas (5GB default limit) and operation statistics tracking using a file-based approach with in-memory caching for performance.

## Design Principles

1. **In-memory quota cache**: Keep quota in memory, flush to disk periodically
2. **Write coalescing**: Batch quota updates, write at most once per second
3. **Time-based stats files**: Store stats in `.stats/YYYY-MM.json` format
4. **Lazy initialization**: Generate `.quota` by scanning FS only when missing
5. **No external dependencies**: Pure file-based solution

## File Structure

```
/s3/
├── bucket1/
│   ├── .quota                  # Current quota/usage
│   ├── .stats/
│   │   ├── 2025-01.json       # January 2025 stats
│   │   ├── 2025-02.json       # February 2025 stats
│   │   └── 2025-09.json       # Current month stats
│   ├── object1.dat
│   └── object2.dat
```

## Implementation Tasks

### Phase 1: Core Data Structures

- [ ] **Add quota structures to `src/models.rs`**
  ```rust
  pub struct BucketQuotaCache {
      pub quota: BucketQuota,
      pub dirty: bool,                    // Needs flush to disk
      pub last_flush: Instant,            // Last time written to disk
  }

  pub struct BucketQuota {
      pub max_size_bytes: u64,            // Default: 5GB
      pub current_usage_bytes: u64,
      pub object_count: u64,
      pub last_updated: DateTime<Utc>,
  }

  pub struct BucketStats {
      pub get_count: u64,
      pub put_count: u64,
      pub delete_count: u64,
      pub list_count: u64,
      pub head_count: u64,
      pub multipart_count: u64,
  }
  ```

- [ ] **Add quota cache to AppState**
  - Add `quota_manager: Arc<QuotaManager>` field
  - Initialize on startup

### Phase 2: Quota Manager Module

- [ ] **Create `src/quota.rs` module**
  ```rust
  pub struct QuotaManager {
      cache: Arc<RwLock<HashMap<String, BucketQuotaCache>>>,
      stats_cache: Arc<RwLock<HashMap<String, BucketStats>>>,
      flush_interval: Duration,  // 1 second
  }
  ```

- [ ] **Implement core methods**
  - [ ] `load_quota(bucket: &str) -> Result<BucketQuota>`
  - [ ] `save_quota(bucket: &str, quota: &BucketQuota) -> Result<()>`
  - [ ] `generate_quota_from_fs(bucket_path: &Path) -> BucketQuota`
  - [ ] `check_quota(bucket: &str, new_size: u64) -> Result<bool>`
  - [ ] `update_quota_add(bucket: &str, size: u64) -> Result<()>`
  - [ ] `update_quota_remove(bucket: &str, size: u64) -> Result<()>`

- [ ] **Implement stats methods**
  - [ ] `get_current_stats_file(bucket: &str) -> PathBuf`
  - [ ] `load_stats(bucket: &str, year_month: &str) -> Result<BucketStats>`
  - [ ] `save_stats(bucket: &str, stats: &BucketStats) -> Result<()>`
  - [ ] `increment_stat(bucket: &str, operation: Operation) -> Result<()>`

### Phase 3: Background Flush Task

- [ ] **Create flush task in `src/quota.rs`**
  ```rust
  async fn quota_flush_task(quota_manager: Arc<QuotaManager>) {
      loop {
          tokio::time::sleep(Duration::from_secs(1)).await;

          // Flush all dirty quotas to disk
          for (bucket, cache) in dirty_quotas {
              write_quota_to_disk(&bucket, &cache.quota).await;
              cache.dirty = false;
          }

          // Flush stats similarly
          flush_stats_to_disk().await;
      }
  }
  ```

- [ ] **Spawn task in `main.rs`**
  - Start flush task after initializing QuotaManager
  - Handle graceful shutdown

### Phase 4: Handler Integration

- [ ] **Modify PUT handler (`src/handlers/object.rs`)**
  ```rust
  // Before upload:
  let quota_ok = state.quota_manager.check_quota(&bucket, content_length).await?;
  if !quota_ok {
      return Err((StatusCode::INSUFFICIENT_STORAGE, "Bucket quota exceeded"));
  }

  // After successful upload:
  state.quota_manager.update_quota_add(&bucket, actual_size).await?;
  state.quota_manager.increment_stat(&bucket, Operation::Put).await?;
  ```

- [ ] **Modify DELETE handler**
  ```rust
  // After successful delete:
  state.quota_manager.update_quota_remove(&bucket, object_size).await?;
  state.quota_manager.increment_stat(&bucket, Operation::Delete).await?;
  ```

- [ ] **Modify GET handler**
  ```rust
  // After serving object:
  state.quota_manager.increment_stat(&bucket, Operation::Get).await?;
  ```

- [ ] **Modify LIST handler**
  ```rust
  state.quota_manager.increment_stat(&bucket, Operation::List).await?;
  ```

- [ ] **Modify HEAD handler**
  ```rust
  state.quota_manager.increment_stat(&bucket, Operation::Head).await?;
  ```

- [ ] **Modify multipart upload handlers**
  - Check quota before completing multipart upload
  - Update quota after successful completion
  - Track multipart stats

### Phase 5: Filesystem Scan Implementation

- [ ] **Implement FS scanner for missing `.quota`**
  ```rust
  fn generate_quota_from_fs(bucket_path: &Path) -> BucketQuota {
      let mut total_size = 0u64;
      let mut object_count = 0u64;

      // Walk directory tree
      for entry in WalkDir::new(bucket_path) {
          if entry.is_file() {
              // Skip hidden files (.quota, .stats/, .policy, etc.)
              if !entry.file_name().starts_with('.') {
                  total_size += entry.metadata()?.len();
                  object_count += 1;
              }
          }
      }

      BucketQuota {
          max_size_bytes: 5 * 1024 * 1024 * 1024, // 5GB default
          current_usage_bytes: total_size,
          object_count,
          last_updated: Utc::now(),
      }
  }
  ```

### Phase 6: Crash Recovery

- [ ] **Implement startup validation**
  ```rust
  // On startup, for each bucket:
  async fn validate_quotas(storage_path: &Path) {
      for bucket in list_buckets(storage_path) {
          let quota_file = bucket.join(".quota");

          if !quota_file.exists() {
              // Generate from FS scan
              let quota = generate_quota_from_fs(&bucket);
              save_quota(&bucket, &quota)?;
          } else {
              // Quick sanity check
              let quota = load_quota(&bucket)?;
              // Sample a few objects to verify consistency
              // If majorly off, regenerate
          }
      }
  }
  ```

### Phase 7: API Enhancements

- [ ] **Add quota headers to responses**
  - `X-Amz-Bucket-Usage`: Current usage in bytes
  - `X-Amz-Bucket-Quota`: Maximum quota in bytes
  - `X-Amz-Bucket-Objects`: Number of objects

- [ ] **Add quota/stats endpoints**
  - `GET /bucket/?quota` - Return current quota info
  - `GET /bucket/?stats` - Return current month stats
  - `GET /bucket/?stats&month=2025-09` - Return specific month stats

### Phase 8: Cluster Mode Considerations

- [ ] **Per-node quota tracking**
  - Each node maintains independent `.quota` and `.stats/`
  - Quota checked before accepting replication
  - Update local quota after successful replication

- [ ] **Stats aggregation (optional)**
  - Implement cluster-wide stats endpoint
  - Aggregate stats from all nodes
  - Cache aggregated results

### Phase 9: Performance Optimizations

- [ ] **Memory management**
  - Implement LRU eviction for inactive bucket quotas
  - Maximum cache size limits
  - Lazy loading of quotas

- [ ] **Atomic file operations**
  - Write to temp file then rename
  - Use file locks if needed
  - Handle concurrent access

### Phase 10: Testing

- [ ] **Unit tests**
  - Quota calculation accuracy
  - Stats increment logic
  - File format parsing/writing

- [ ] **Integration tests**
  - Quota enforcement on uploads
  - Stats tracking accuracy
  - Crash recovery scenarios

- [ ] **Performance tests**
  - High concurrency quota updates
  - Large bucket FS scan time
  - Memory usage under load

## File Formats

### `.quota` file
```json
{
  "max_size_bytes": 5368709120,
  "current_usage_bytes": 1234567890,
  "object_count": 42,
  "last_updated": "2025-01-27T10:00:00Z"
}
```

### `.stats/YYYY-MM.json` file
```json
{
  "get_count": 50000,
  "put_count": 10000,
  "delete_count": 2000,
  "list_count": 5000,
  "head_count": 3000,
  "multipart_count": 500,
  "last_updated": "2025-09-27T10:00:00Z"
}
```

## Error Handling

- **Corrupted `.quota` file**: Regenerate from FS scan
- **Missing stats file**: Create new with zero counts
- **Over-quota during race**: Allow slight overage, block next upload
- **Flush task failure**: Log error, retry next interval
- **507 Insufficient Storage**: Return when quota exceeded

## Configuration

Environment variables:
- `BUCKET_QUOTA_BYTES`: Override default 5GB limit
- `QUOTA_FLUSH_INTERVAL_MS`: Override 1 second flush interval
- `QUOTA_CACHE_MAX_SIZE`: Maximum number of buckets in cache

## Performance Targets

- Quota check: < 1ms (from memory)
- Stats increment: < 1ms (memory only)
- Quota flush: < 10ms per bucket
- FS scan (1M objects): < 30 seconds
- Memory overhead: < 1KB per bucket

## Migration Notes

- Existing buckets will generate `.quota` on first access
- No data migration required
- Stats start accumulating from deployment date
- Backward compatible with existing IronBucket deployments