use super::{BucketInfo, ListObjectsResult, ObjectMetadata, StorageBackend};
use crate::{config::StorageConfig, error::{Error, Result}};
use async_trait::async_trait;
use chrono::Utc;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tokio::{
    fs,
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
};
use tracing::{debug, error, info};

pub struct FileSystemBackend {
    base_path: PathBuf,
    config: StorageConfig,
}

impl FileSystemBackend {
    pub fn new(config: &StorageConfig) -> Result<Self> {
        let base_path = config.path.clone();

        // Create base directory if it doesn't exist
        std::fs::create_dir_all(&base_path)?;

        Ok(Self {
            base_path,
            config: config.clone(),
        })
    }

    fn bucket_path(&self, bucket: &str) -> PathBuf {
        self.base_path.join(bucket)
    }

    fn object_path(&self, bucket: &str, key: &str) -> PathBuf {
        self.bucket_path(bucket).join(key)
    }

    fn metadata_path(&self, bucket: &str, key: &str) -> PathBuf {
        let mut path = self.object_path(bucket, key);
        path.set_extension("metadata");
        path
    }

    async fn save_metadata(&self, bucket: &str, key: &str, metadata: &ObjectMetadata) -> Result<()> {
        let path = self.metadata_path(bucket, key);
        let json = serde_json::to_string(metadata)?;
        fs::write(path, json).await?;
        Ok(())
    }

    async fn load_metadata(&self, bucket: &str, key: &str) -> Result<ObjectMetadata> {
        let path = self.metadata_path(bucket, key);
        let json = fs::read_to_string(path).await?;
        let metadata = serde_json::from_str(&json)?;
        Ok(metadata)
    }

    fn calculate_etag(data: &[u8]) -> String {
        format!("{:x}", md5::compute(data))
    }
}

#[async_trait]
impl StorageBackend for FileSystemBackend {
    async fn create_bucket(&self, bucket: &str) -> Result<()> {
        let path = self.bucket_path(bucket);

        if path.exists() {
            return Err(Error::BucketAlreadyExists);
        }

        fs::create_dir_all(&path).await?;

        // Create bucket metadata
        let info = BucketInfo {
            name: bucket.to_string(),
            created: Utc::now(),
            region: "us-east-1".to_string(),
        };

        let metadata_path = path.join(".bucket_info");
        let json = serde_json::to_string(&info)?;
        fs::write(metadata_path, json).await?;

        info!("Created bucket: {}", bucket);
        Ok(())
    }

    async fn delete_bucket(&self, bucket: &str) -> Result<()> {
        let path = self.bucket_path(bucket);

        if !path.exists() {
            return Err(Error::NoSuchBucket);
        }

        // Check if bucket is empty
        let mut entries = fs::read_dir(&path).await?;
        let mut count = 0;
        while let Some(_) = entries.next_entry().await? {
            count += 1;
            if count > 1 { // More than just .bucket_info
                return Err(Error::BucketNotEmpty);
            }
        }

        fs::remove_dir_all(&path).await?;
        info!("Deleted bucket: {}", bucket);
        Ok(())
    }

    async fn list_buckets(&self) -> Result<Vec<BucketInfo>> {
        let mut buckets = Vec::new();
        let mut entries = fs::read_dir(&self.base_path).await?;

        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let bucket_info_path = entry.path().join(".bucket_info");
                if bucket_info_path.exists() {
                    let json = fs::read_to_string(bucket_info_path).await?;
                    if let Ok(info) = serde_json::from_str(&json) {
                        buckets.push(info);
                    }
                }
            }
        }

        Ok(buckets)
    }

    async fn bucket_exists(&self, bucket: &str) -> Result<bool> {
        Ok(self.bucket_path(bucket).exists())
    }

    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        data: Vec<u8>,
        mut metadata: HashMap<String, String>,
    ) -> Result<ObjectMetadata> {
        if !self.bucket_exists(bucket).await? {
            return Err(Error::NoSuchBucket);
        }

        let object_path = self.object_path(bucket, key);

        // Create parent directories if needed
        if let Some(parent) = object_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Write object data
        fs::write(&object_path, &data).await?;

        // Create object metadata
        let etag = Self::calculate_etag(&data);
        let content_type = metadata
            .get("content-type")
            .cloned()
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let object_metadata = ObjectMetadata {
            key: key.to_string(),
            size: data.len() as u64,
            etag: etag.clone(),
            last_modified: Utc::now(),
            content_type,
            storage_class: "STANDARD".to_string(),
            metadata,
            version_id: None,
        };

        // Save metadata
        self.save_metadata(bucket, key, &object_metadata).await?;

        debug!("Stored object: {}/{} (size: {} bytes)", bucket, key, data.len());
        Ok(object_metadata)
    }

    async fn get_object(&self, bucket: &str, key: &str) -> Result<(Vec<u8>, ObjectMetadata)> {
        if !self.bucket_exists(bucket).await? {
            return Err(Error::NoSuchBucket);
        }

        let object_path = self.object_path(bucket, key);
        if !object_path.exists() {
            return Err(Error::NoSuchKey);
        }

        let data = fs::read(&object_path).await?;
        let metadata = self.load_metadata(bucket, key).await?;

        Ok((data, metadata))
    }

    async fn get_object_stream(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<(Box<dyn AsyncRead + Send + Unpin>, ObjectMetadata)> {
        if !self.bucket_exists(bucket).await? {
            return Err(Error::NoSuchBucket);
        }

        let object_path = self.object_path(bucket, key);
        if !object_path.exists() {
            return Err(Error::NoSuchKey);
        }

        let file = fs::File::open(&object_path).await?;
        let metadata = self.load_metadata(bucket, key).await?;

        Ok((Box::new(file), metadata))
    }

    async fn delete_object(&self, bucket: &str, key: &str) -> Result<()> {
        if !self.bucket_exists(bucket).await? {
            return Err(Error::NoSuchBucket);
        }

        let object_path = self.object_path(bucket, key);
        if !object_path.exists() {
            return Err(Error::NoSuchKey);
        }

        fs::remove_file(&object_path).await?;

        // Remove metadata
        let metadata_path = self.metadata_path(bucket, key);
        if metadata_path.exists() {
            fs::remove_file(metadata_path).await?;
        }

        debug!("Deleted object: {}/{}", bucket, key);
        Ok(())
    }

    async fn head_object(&self, bucket: &str, key: &str) -> Result<ObjectMetadata> {
        if !self.bucket_exists(bucket).await? {
            return Err(Error::NoSuchBucket);
        }

        let object_path = self.object_path(bucket, key);
        if !object_path.exists() {
            return Err(Error::NoSuchKey);
        }

        self.load_metadata(bucket, key).await
    }

    async fn copy_object(
        &self,
        source_bucket: &str,
        source_key: &str,
        dest_bucket: &str,
        dest_key: &str,
        metadata: HashMap<String, String>,
    ) -> Result<ObjectMetadata> {
        let (data, _) = self.get_object(source_bucket, source_key).await?;
        self.put_object(dest_bucket, dest_key, data, metadata).await
    }

    async fn list_objects(
        &self,
        bucket: &str,
        prefix: Option<&str>,
        delimiter: Option<&str>,
        continuation_token: Option<&str>,
        max_keys: usize,
    ) -> Result<ListObjectsResult> {
        if !self.bucket_exists(bucket).await? {
            return Err(Error::NoSuchBucket);
        }

        let bucket_path = self.bucket_path(bucket);
        let mut objects = Vec::new();
        let mut prefixes = Vec::new();

        // Simple implementation - can be optimized with pagination
        self.list_objects_recursive(
            &bucket_path,
            &bucket_path,
            prefix,
            delimiter,
            &mut objects,
            &mut prefixes,
            max_keys,
        ).await?;

        Ok(ListObjectsResult {
            objects,
            prefixes,
            is_truncated: false,
            next_continuation_token: None,
        })
    }

    async fn get_bucket_size(&self, bucket: &str) -> Result<u64> {
        if !self.bucket_exists(bucket).await? {
            return Err(Error::NoSuchBucket);
        }

        let bucket_path = self.bucket_path(bucket);
        let mut total_size = 0u64;

        self.calculate_size_recursive(&bucket_path, &mut total_size).await?;

        Ok(total_size)
    }

    async fn get_object_count(&self, bucket: &str) -> Result<usize> {
        if !self.bucket_exists(bucket).await? {
            return Err(Error::NoSuchBucket);
        }

        let bucket_path = self.bucket_path(bucket);
        let mut count = 0;

        self.count_objects_recursive(&bucket_path, &mut count).await?;

        Ok(count)
    }
}

impl FileSystemBackend {
    async fn list_objects_recursive(
        &self,
        base_path: &Path,
        current_path: &Path,
        prefix: Option<&str>,
        delimiter: Option<&str>,
        objects: &mut Vec<ObjectMetadata>,
        prefixes: &mut Vec<String>,
        max_keys: usize,
    ) -> Result<()> {
        let mut entries = fs::read_dir(current_path).await?;

        while let Some(entry) = entries.next_entry().await? {
            if objects.len() >= max_keys {
                break;
            }

            let path = entry.path();
            let relative_path = path.strip_prefix(base_path).unwrap();
            let key = relative_path.to_string_lossy().to_string();

            // Skip metadata files
            if key.ends_with(".metadata") || key.starts_with(".") {
                continue;
            }

            // Check prefix filter
            if let Some(p) = prefix {
                if !key.starts_with(p) {
                    continue;
                }
            }

            if entry.file_type().await?.is_file() {
                // Try to load metadata, skip if it fails
                if let Ok(metadata) = self.load_metadata(
                    base_path.file_name().unwrap().to_str().unwrap(),
                    &key
                ).await {
                    objects.push(metadata);
                }
            } else if entry.file_type().await?.is_dir() && delimiter.is_none() {
                // Recursively list subdirectories when no delimiter
                Box::pin(self.list_objects_recursive(
                    base_path,
                    &path,
                    prefix,
                    delimiter,
                    objects,
                    prefixes,
                    max_keys,
                )).await?;
            }
        }

        Ok(())
    }

    async fn calculate_size_recursive(&self, path: &Path, total: &mut u64) -> Result<()> {
        let mut entries = fs::read_dir(path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let metadata = entry.metadata().await?;

            if metadata.is_file() && !path.to_string_lossy().ends_with(".metadata") {
                *total += metadata.len();
            } else if metadata.is_dir() {
                Box::pin(self.calculate_size_recursive(&path, total)).await?;
            }
        }

        Ok(())
    }

    async fn count_objects_recursive(&self, path: &Path, count: &mut usize) -> Result<()> {
        let mut entries = fs::read_dir(path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let metadata = entry.metadata().await?;

            if metadata.is_file() && !path.to_string_lossy().ends_with(".metadata") {
                *count += 1;
            } else if metadata.is_dir() && !path.file_name().unwrap().to_string_lossy().starts_with(".") {
                Box::pin(self.count_objects_recursive(&path, count)).await?;
            }
        }

        Ok(())
    }
}