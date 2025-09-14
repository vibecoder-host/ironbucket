pub mod filesystem;

use crate::error::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::io::AsyncRead;

pub use filesystem::FileSystemBackend;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectMetadata {
    pub key: String,
    pub size: u64,
    pub etag: String,
    pub last_modified: DateTime<Utc>,
    pub content_type: String,
    pub storage_class: String,
    pub metadata: HashMap<String, String>,
    pub version_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketInfo {
    pub name: String,
    pub created: DateTime<Utc>,
    pub region: String,
}

#[derive(Debug, Clone)]
pub struct ListObjectsResult {
    pub objects: Vec<ObjectMetadata>,
    pub prefixes: Vec<String>,
    pub is_truncated: bool,
    pub next_continuation_token: Option<String>,
}

#[async_trait]
pub trait StorageBackend: Send + Sync {
    // Bucket operations
    async fn create_bucket(&self, bucket: &str) -> Result<()>;
    async fn delete_bucket(&self, bucket: &str) -> Result<()>;
    async fn list_buckets(&self) -> Result<Vec<BucketInfo>>;
    async fn bucket_exists(&self, bucket: &str) -> Result<bool>;

    // Object operations
    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        data: Vec<u8>,
        metadata: HashMap<String, String>,
    ) -> Result<ObjectMetadata>;

    async fn get_object(&self, bucket: &str, key: &str) -> Result<(Vec<u8>, ObjectMetadata)>;

    async fn get_object_stream(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<(Box<dyn AsyncRead + Send + Unpin>, ObjectMetadata)>;

    async fn delete_object(&self, bucket: &str, key: &str) -> Result<()>;

    async fn head_object(&self, bucket: &str, key: &str) -> Result<ObjectMetadata>;

    async fn copy_object(
        &self,
        source_bucket: &str,
        source_key: &str,
        dest_bucket: &str,
        dest_key: &str,
        metadata: HashMap<String, String>,
    ) -> Result<ObjectMetadata>;

    async fn list_objects(
        &self,
        bucket: &str,
        prefix: Option<&str>,
        delimiter: Option<&str>,
        continuation_token: Option<&str>,
        max_keys: usize,
    ) -> Result<ListObjectsResult>;

    // Storage stats
    async fn get_bucket_size(&self, bucket: &str) -> Result<u64>;
    async fn get_object_count(&self, bucket: &str) -> Result<usize>;
}