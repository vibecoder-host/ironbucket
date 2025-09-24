use std::fs;
use std::path::PathBuf;
use chrono::{DateTime, Utc};
// Note: serde imports removed as they're not needed
use crate::{BucketEncryption, CorsConfiguration, LifecycleConfiguration};

/// Check if a bucket exists on the filesystem
pub fn bucket_exists(storage_path: &PathBuf, bucket: &str) -> bool {
    let bucket_path = storage_path.join(bucket);
    bucket_path.exists() && bucket_path.is_dir()
}

/// Get bucket creation time from filesystem
pub fn get_bucket_created_time(storage_path: &PathBuf, bucket: &str) -> Option<DateTime<Utc>> {
    let bucket_path = storage_path.join(bucket);
    if let Ok(metadata) = fs::metadata(&bucket_path) {
        if let Ok(created) = metadata.created() {
            if let Some(datetime) = DateTime::from_timestamp(
                created.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64,
                0
            ) {
                return Some(datetime);
            }
        }
    }
    None
}

/// Read bucket policy from filesystem
pub fn read_bucket_policy(storage_path: &PathBuf, bucket: &str) -> Option<String> {
    let policy_file = storage_path.join(bucket).join(".policy");
    if policy_file.exists() {
        fs::read_to_string(&policy_file).ok()
    } else {
        None
    }
}

/// Write bucket policy to filesystem
pub fn write_bucket_policy(storage_path: &PathBuf, bucket: &str, policy: &str) -> Result<(), std::io::Error> {
    let policy_file = storage_path.join(bucket).join(".policy");
    fs::write(&policy_file, policy)
}

/// Delete bucket policy from filesystem
pub fn delete_bucket_policy(storage_path: &PathBuf, bucket: &str) -> Result<(), std::io::Error> {
    let policy_file = storage_path.join(bucket).join(".policy");
    if policy_file.exists() {
        fs::remove_file(&policy_file)
    } else {
        Ok(())
    }
}

/// Read bucket encryption from filesystem
pub fn read_bucket_encryption(storage_path: &PathBuf, bucket: &str) -> Option<BucketEncryption> {
    let encryption_file = storage_path.join(bucket).join(".encryption");
    if encryption_file.exists() {
        if let Ok(encryption_json) = fs::read_to_string(&encryption_file) {
            serde_json::from_str::<BucketEncryption>(&encryption_json).ok()
        } else {
            None
        }
    } else {
        None
    }
}

/// Write bucket encryption to filesystem
pub fn write_bucket_encryption(storage_path: &PathBuf, bucket: &str, encryption: &BucketEncryption) -> Result<(), Box<dyn std::error::Error>> {
    let encryption_file = storage_path.join(bucket).join(".encryption");
    let encryption_json = serde_json::to_string_pretty(encryption)?;
    fs::write(&encryption_file, encryption_json)?;
    Ok(())
}

/// Delete bucket encryption from filesystem
pub fn delete_bucket_encryption(storage_path: &PathBuf, bucket: &str) -> Result<(), std::io::Error> {
    let encryption_file = storage_path.join(bucket).join(".encryption");
    if encryption_file.exists() {
        fs::remove_file(&encryption_file)
    } else {
        Ok(())
    }
}

/// Read bucket CORS configuration from filesystem
pub fn read_bucket_cors(storage_path: &PathBuf, bucket: &str) -> Option<CorsConfiguration> {
    let cors_file = storage_path.join(bucket).join(".cors");
    if cors_file.exists() {
        if let Ok(cors_json) = fs::read_to_string(&cors_file) {
            serde_json::from_str::<CorsConfiguration>(&cors_json).ok()
        } else {
            None
        }
    } else {
        None
    }
}

/// Write bucket CORS configuration to filesystem
pub fn write_bucket_cors(storage_path: &PathBuf, bucket: &str, cors: &CorsConfiguration) -> Result<(), Box<dyn std::error::Error>> {
    let cors_file = storage_path.join(bucket).join(".cors");
    let cors_json = serde_json::to_string_pretty(cors)?;
    fs::write(&cors_file, cors_json)?;
    Ok(())
}

/// Delete bucket CORS configuration from filesystem
pub fn delete_bucket_cors(storage_path: &PathBuf, bucket: &str) -> Result<(), std::io::Error> {
    let cors_file = storage_path.join(bucket).join(".cors");
    if cors_file.exists() {
        fs::remove_file(&cors_file)
    } else {
        Ok(())
    }
}

/// Read bucket lifecycle configuration from filesystem
pub fn read_bucket_lifecycle(storage_path: &PathBuf, bucket: &str) -> Option<LifecycleConfiguration> {
    let lifecycle_file = storage_path.join(bucket).join(".lifecycle");
    if lifecycle_file.exists() {
        if let Ok(lifecycle_json) = fs::read_to_string(&lifecycle_file) {
            serde_json::from_str::<LifecycleConfiguration>(&lifecycle_json).ok()
        } else {
            None
        }
    } else {
        None
    }
}

/// Write bucket lifecycle configuration to filesystem
pub fn write_bucket_lifecycle(storage_path: &PathBuf, bucket: &str, lifecycle: &LifecycleConfiguration) -> Result<(), Box<dyn std::error::Error>> {
    let lifecycle_file = storage_path.join(bucket).join(".lifecycle");
    let lifecycle_json = serde_json::to_string_pretty(lifecycle)?;
    fs::write(&lifecycle_file, lifecycle_json)?;
    Ok(())
}

/// Delete bucket lifecycle configuration from filesystem
pub fn delete_bucket_lifecycle(storage_path: &PathBuf, bucket: &str) -> Result<(), std::io::Error> {
    let lifecycle_file = storage_path.join(bucket).join(".lifecycle");
    if lifecycle_file.exists() {
        fs::remove_file(&lifecycle_file)
    } else {
        Ok(())
    }
}

/// Read bucket versioning status from filesystem
pub fn read_bucket_versioning(storage_path: &PathBuf, bucket: &str) -> Option<String> {
    let versioning_file = storage_path.join(bucket).join(".versioning");
    if versioning_file.exists() {
        fs::read_to_string(&versioning_file).ok()
    } else {
        None
    }
}

/// Write bucket versioning status to filesystem
pub fn write_bucket_versioning(storage_path: &PathBuf, bucket: &str, status: &str) -> Result<(), std::io::Error> {
    let versioning_file = storage_path.join(bucket).join(".versioning");
    fs::write(&versioning_file, status)
}

/// List all buckets from filesystem
pub fn list_bucket_names(storage_path: &PathBuf) -> Result<Vec<String>, std::io::Error> {
    let mut buckets = Vec::new();

    if let Ok(entries) = fs::read_dir(storage_path) {
        for entry in entries {
            if let Ok(entry) = entry {
                if entry.path().is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        // Skip hidden directories and system directories
                        if !name.starts_with('.') {
                            buckets.push(name.to_string());
                        }
                    }
                }
            }
        }
    }

    buckets.sort();
    Ok(buckets)
}