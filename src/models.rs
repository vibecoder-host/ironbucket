use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone)]
pub struct AppState {
    pub storage_path: PathBuf,
    pub access_keys: Arc<HashMap<String, String>>,
    pub multipart_uploads: Arc<Mutex<HashMap<String, MultipartUpload>>>,
    pub quota_manager: Arc<crate::quota::QuotaManager>,
    pub wal_writer: Arc<crate::wal::WALWriter>,
}

#[derive(Clone)]
pub struct BucketData {
    pub created: DateTime<Utc>,
    pub objects: HashMap<String, ObjectData>,
    pub versioning_status: Option<String>, // "Enabled", "Suspended", or None
    pub versions: HashMap<String, Vec<ObjectVersion>>, // key -> list of versions
    pub policy: Option<String>, // JSON policy document
    pub encryption: Option<BucketEncryption>, // Bucket encryption configuration
    pub cors: Option<CorsConfiguration>, // CORS configuration
    pub lifecycle: Option<LifecycleConfiguration>, // Lifecycle configuration
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BucketEncryption {
    pub algorithm: String, // "AES256" or "aws:kms"
    pub kms_key_id: Option<String>, // KMS key ID if using KMS
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CorsConfiguration {
    #[serde(rename = "CORSRules")]
    pub cors_rules: Vec<CorsRule>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CorsRule {
    #[serde(rename = "AllowedHeaders", skip_serializing_if = "Option::is_none")]
    pub allowed_headers: Option<Vec<String>>,
    #[serde(rename = "AllowedMethods")]
    pub allowed_methods: Vec<String>,
    #[serde(rename = "AllowedOrigins")]
    pub allowed_origins: Vec<String>,
    #[serde(rename = "ExposeHeaders", skip_serializing_if = "Option::is_none")]
    pub expose_headers: Option<Vec<String>>,
    #[serde(rename = "MaxAgeSeconds", skip_serializing_if = "Option::is_none")]
    pub max_age_seconds: Option<u32>,
    #[serde(rename = "ID", skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

// Lifecycle configuration structures
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LifecycleConfiguration {
    #[serde(rename = "Rules")]
    pub rules: Vec<LifecycleRule>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LifecycleRule {
    #[serde(rename = "ID", skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "Status")]
    pub status: String, // "Enabled" or "Disabled"
    #[serde(rename = "Filter", skip_serializing_if = "Option::is_none")]
    pub filter: Option<LifecycleFilter>,
    #[serde(rename = "Transitions", skip_serializing_if = "Option::is_none")]
    pub transitions: Option<Vec<LifecycleTransition>>,
    #[serde(rename = "Expiration", skip_serializing_if = "Option::is_none")]
    pub expiration: Option<LifecycleExpiration>,
    #[serde(rename = "NoncurrentVersionTransitions", skip_serializing_if = "Option::is_none")]
    pub noncurrent_version_transitions: Option<Vec<NoncurrentVersionTransition>>,
    #[serde(rename = "NoncurrentVersionExpiration", skip_serializing_if = "Option::is_none")]
    pub noncurrent_version_expiration: Option<NoncurrentVersionExpiration>,
    #[serde(rename = "AbortIncompleteMultipartUpload", skip_serializing_if = "Option::is_none")]
    pub abort_incomplete_multipart_upload: Option<AbortIncompleteMultipartUpload>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LifecycleFilter {
    #[serde(rename = "Prefix", skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(rename = "Tag", skip_serializing_if = "Option::is_none")]
    pub tag: Option<LifecycleTag>,
    #[serde(rename = "And", skip_serializing_if = "Option::is_none")]
    pub and: Option<LifecycleAnd>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LifecycleAnd {
    #[serde(rename = "Prefix", skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(rename = "Tags", skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<LifecycleTag>>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LifecycleTag {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "Value")]
    pub value: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LifecycleTransition {
    #[serde(rename = "Days", skip_serializing_if = "Option::is_none")]
    pub days: Option<u32>,
    #[serde(rename = "Date", skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(rename = "StorageClass")]
    pub storage_class: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LifecycleExpiration {
    #[serde(rename = "Days", skip_serializing_if = "Option::is_none")]
    pub days: Option<u32>,
    #[serde(rename = "Date", skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(rename = "ExpiredObjectDeleteMarker", skip_serializing_if = "Option::is_none")]
    pub expired_object_delete_marker: Option<bool>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct NoncurrentVersionTransition {
    #[serde(rename = "NoncurrentDays")]
    pub noncurrent_days: u32,
    #[serde(rename = "StorageClass")]
    pub storage_class: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct NoncurrentVersionExpiration {
    #[serde(rename = "NoncurrentDays")]
    pub noncurrent_days: u32,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AbortIncompleteMultipartUpload {
    #[serde(rename = "DaysAfterInitiation")]
    pub days_after_initiation: u32,
}

#[derive(Clone)]
pub struct ObjectData {
    pub data: Vec<u8>,
    pub etag: String,
    pub last_modified: DateTime<Utc>,
    pub size: usize,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ObjectVersion {
    pub version_id: String,
    pub data: Vec<u8>,
    pub etag: String,
    pub last_modified: DateTime<Utc>,
    pub size: usize,
    pub is_latest: bool,
    pub is_delete_marker: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ObjectMetadata {
    pub key: String,
    pub size: u64,
    pub etag: String,
    pub last_modified: DateTime<Utc>,
    pub content_type: String,
    pub storage_class: String,
    pub metadata: HashMap<String, String>,
    pub version_id: Option<String>,
    pub encryption: Option<ObjectEncryption>,
    pub tags: Option<HashMap<String, String>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ObjectEncryption {
    pub algorithm: String,
    pub key_base64: String, // Base64 encoded encryption key
    pub nonce_base64: String, // Base64 encoded nonce for GCM
}

#[derive(Clone)]
pub struct MultipartUpload {
    pub upload_id: String,
    pub bucket: String,
    pub key: String,
    pub parts: HashMap<i32, UploadPart>,
    pub initiated: DateTime<Utc>,
}

#[derive(Clone)]
pub struct UploadPart {
    pub part_number: i32,
    pub etag: String,
    pub size: usize,
    pub data: Vec<u8>,
}

#[derive(Deserialize, Debug)]
pub struct BucketQueryParams {
    pub location: Option<String>,
    pub versioning: Option<String>,
    pub versions: Option<String>,
    pub acl: Option<String>,
    pub policy: Option<String>,
    pub encryption: Option<String>,
    pub cors: Option<String>,
    pub lifecycle: Option<String>,
    pub uploads: Option<String>,
    pub delete: Option<String>,
    pub quota: Option<String>,
    pub stats: Option<String>,
    pub month: Option<String>,
    #[serde(rename = "max-keys")]
    pub max_keys: Option<usize>,
    pub prefix: Option<String>,
    #[serde(rename = "continuation-token")]
    pub continuation_token: Option<String>,
    pub delimiter: Option<String>,
    #[serde(rename = "list-type")]
    pub list_type: Option<String>,
    #[serde(rename = "version-id-marker")]
    pub version_id_marker: Option<String>,
    #[serde(rename = "key-marker")]
    pub key_marker: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct ObjectQueryParams {
    pub uploads: Option<String>,
    #[serde(rename = "uploadId")]
    pub upload_id: Option<String>,
    #[serde(rename = "partNumber")]
    pub part_number: Option<i32>,
    pub acl: Option<String>,
    pub versions: Option<String>,
    #[serde(rename = "versionId")]
    pub version_id: Option<String>,
    pub tagging: Option<String>,
}

// Quota and Stats structures
#[derive(Clone, Debug)]
pub struct BucketQuotaCache {
    pub quota: BucketQuota,
    pub dirty: bool,                    // Needs flush to disk
    pub last_flush: Instant,            // Last time written to disk
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct BucketQuota {
    pub max_size_bytes: u64,            // Default: 5GB
    pub current_usage_bytes: u64,
    pub object_count: u64,
    pub last_updated: DateTime<Utc>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct BucketStats {
    pub get_count: u64,
    pub put_count: u64,
    pub delete_count: u64,
    pub list_count: u64,
    pub head_count: u64,
    pub multipart_count: u64,
}

#[derive(Clone, Copy, Debug)]
pub enum Operation {
    Get,
    Put,
    Delete,
    List,
    Head,
    Multipart,
}