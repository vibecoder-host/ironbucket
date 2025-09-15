use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{header, HeaderMap, StatusCode, Method, Request},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, head, post, put},
    Router,
};
use bytes::Bytes;
use chrono::{Utc, DateTime};
use serde::{Deserialize, Serialize};
use serde_json;
use std::{
    collections::HashMap,
    env,
    fs,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tower_http::cors::CorsLayer;
use tracing::{info, debug, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use hmac::Hmac;
use sha2::Sha256;
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce, Key,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use rand::RngCore;
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    storage_path: PathBuf,
    buckets: Arc<Mutex<HashMap<String, BucketData>>>,
    access_keys: Arc<HashMap<String, String>>,
    multipart_uploads: Arc<Mutex<HashMap<String, MultipartUpload>>>,
}

type HmacSha256 = Hmac<Sha256>;

// Helper function to format date for HTTP Last-Modified header (RFC2822 with GMT)
fn format_http_date(dt: &DateTime<Utc>) -> String {
    dt.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

// Helper function to parse AWS chunked transfer encoding with signatures
fn parse_chunked_data(input: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut pos = 0;

    while pos < input.len() {
        // Find the chunk size (before semicolon)
        let chunk_header_end = match find_sequence(&input[pos..], b"\r\n") {
            Some(i) => pos + i,
            None => break,
        };

        let header = &input[pos..chunk_header_end];
        let header_str = String::from_utf8_lossy(header);

        // Parse chunk size (hex before semicolon or end of header)
        let size_str = if let Some(semi_pos) = header_str.find(';') {
            &header_str[..semi_pos]
        } else {
            &header_str
        };

        // Parse hex chunk size
        let chunk_size = match usize::from_str_radix(size_str.trim(), 16) {
            Ok(size) => size,
            Err(_) => break,
        };

        // Skip past header and \r\n
        pos = chunk_header_end + 2;

        // If chunk size is 0, we're done
        if chunk_size == 0 {
            break;
        }

        // Read chunk data
        if pos + chunk_size <= input.len() {
            result.extend_from_slice(&input[pos..pos + chunk_size]);
            pos += chunk_size;

            // Skip trailing \r\n after chunk
            if pos + 2 <= input.len() && &input[pos..pos + 2] == b"\r\n" {
                pos += 2;
            }
        } else {
            break;
        }
    }

    // If no chunks were parsed, return original data
    if result.is_empty() {
        input.to_vec()
    } else {
        result
    }
}

// Helper to find a byte sequence in a slice
fn find_sequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len())
        .position(|window| window == needle)
}

// Generate a random 256-bit key for AES encryption
fn generate_encryption_key() -> Vec<u8> {
    let mut key = vec![0u8; 32]; // 256 bits
    OsRng.fill_bytes(&mut key);
    key
}

// Encrypt data using AES-256-GCM
fn encrypt_data(data: &[u8], key: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let key = Key::<Aes256Gcm>::from_slice(key);
    let cipher = Aes256Gcm::new(key);

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    match cipher.encrypt(nonce, data) {
        Ok(ciphertext) => Ok((ciphertext, nonce_bytes.to_vec())),
        Err(e) => Err(format!("Encryption failed: {:?}", e)),
    }
}

// Decrypt data using AES-256-GCM
fn decrypt_data(ciphertext: &[u8], key: &[u8], nonce: &[u8]) -> Result<Vec<u8>, String> {
    let key = Key::<Aes256Gcm>::from_slice(key);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(nonce);

    match cipher.decrypt(nonce, ciphertext) {
        Ok(plaintext) => Ok(plaintext),
        Err(e) => Err(format!("Decryption failed: {:?}", e)),
    }
}

// Check if an action is allowed based on bucket policy
fn check_policy_permission(
    policy_json: &str,
    action: &str,
    resource: &str,
    principal: &str,
) -> bool {
    // Parse the policy
    if let Ok(policy) = serde_json::from_str::<serde_json::Value>(policy_json) {
        if let Some(statements) = policy.get("Statement").and_then(|s| s.as_array()) {
            for statement in statements {
                // Check Effect
                let effect = statement.get("Effect")
                    .and_then(|e| e.as_str())
                    .unwrap_or("");

                // Check Principal
                let principal_match = if let Some(p) = statement.get("Principal") {
                    if p.as_str() == Some("*") || p == "*" {
                        true
                    } else if let Some(aws) = p.get("AWS") {
                        if let Some(arr) = aws.as_array() {
                            arr.iter().any(|v| v.as_str() == Some(principal))
                        } else {
                            aws.as_str() == Some(principal)
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                // Check Action
                let action_match = if let Some(actions) = statement.get("Action") {
                    if let Some(arr) = actions.as_array() {
                        arr.iter().any(|a| {
                            if let Some(act) = a.as_str() {
                                act == action || act == "s3:*" ||
                                (act.ends_with("*") && action.starts_with(&act[..act.len()-1]))
                            } else {
                                false
                            }
                        })
                    } else if let Some(act) = actions.as_str() {
                        act == action || act == "s3:*" ||
                        (act.ends_with("*") && action.starts_with(&act[..act.len()-1]))
                    } else {
                        false
                    }
                } else {
                    false
                };

                // Check Resource
                let resource_match = if let Some(resources) = statement.get("Resource") {
                    if let Some(arr) = resources.as_array() {
                        arr.iter().any(|r| {
                            if let Some(res) = r.as_str() {
                                res == resource || res == "*" ||
                                (res.ends_with("*") && resource.starts_with(&res[..res.len()-1]))
                            } else {
                                false
                            }
                        })
                    } else if let Some(res) = resources.as_str() {
                        res == resource || res == "*" ||
                        (res.ends_with("*") && resource.starts_with(&res[..res.len()-1]))
                    } else {
                        false
                    }
                } else {
                    false
                };

                // If all conditions match
                if principal_match && action_match && resource_match {
                    if effect == "Allow" {
                        return true;
                    } else if effect == "Deny" {
                        return false;
                    }
                }
            }
        }
    }

    // Default deny if no matching statement
    false
}

#[derive(Clone)]
struct BucketData {
    created: chrono::DateTime<Utc>,
    objects: HashMap<String, ObjectData>,
    versioning_status: Option<String>, // "Enabled", "Suspended", or None
    versions: HashMap<String, Vec<ObjectVersion>>, // key -> list of versions
    policy: Option<String>, // JSON policy document
    encryption: Option<BucketEncryption>, // Bucket encryption configuration
    cors: Option<CorsConfiguration>, // CORS configuration
    lifecycle: Option<LifecycleConfiguration>, // Lifecycle configuration
}

#[derive(Clone, Serialize, Deserialize)]
struct BucketEncryption {
    algorithm: String, // "AES256" or "aws:kms"
    kms_key_id: Option<String>, // KMS key ID if using KMS
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct CorsConfiguration {
    #[serde(rename = "CORSRules")]
    cors_rules: Vec<CorsRule>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct CorsRule {
    #[serde(rename = "AllowedHeaders", skip_serializing_if = "Option::is_none")]
    allowed_headers: Option<Vec<String>>,
    #[serde(rename = "AllowedMethods")]
    allowed_methods: Vec<String>,
    #[serde(rename = "AllowedOrigins")]
    allowed_origins: Vec<String>,
    #[serde(rename = "ExposeHeaders", skip_serializing_if = "Option::is_none")]
    expose_headers: Option<Vec<String>>,
    #[serde(rename = "MaxAgeSeconds", skip_serializing_if = "Option::is_none")]
    max_age_seconds: Option<u32>,
    #[serde(rename = "ID", skip_serializing_if = "Option::is_none")]
    id: Option<String>,
}

// Lifecycle configuration structures
#[derive(Clone, Serialize, Deserialize, Debug)]
struct LifecycleConfiguration {
    #[serde(rename = "Rules")]
    rules: Vec<LifecycleRule>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct LifecycleRule {
    #[serde(rename = "ID", skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(rename = "Status")]
    status: String, // "Enabled" or "Disabled"
    #[serde(rename = "Filter", skip_serializing_if = "Option::is_none")]
    filter: Option<LifecycleFilter>,
    #[serde(rename = "Transitions", skip_serializing_if = "Option::is_none")]
    transitions: Option<Vec<LifecycleTransition>>,
    #[serde(rename = "Expiration", skip_serializing_if = "Option::is_none")]
    expiration: Option<LifecycleExpiration>,
    #[serde(rename = "NoncurrentVersionTransitions", skip_serializing_if = "Option::is_none")]
    noncurrent_version_transitions: Option<Vec<NoncurrentVersionTransition>>,
    #[serde(rename = "NoncurrentVersionExpiration", skip_serializing_if = "Option::is_none")]
    noncurrent_version_expiration: Option<NoncurrentVersionExpiration>,
    #[serde(rename = "AbortIncompleteMultipartUpload", skip_serializing_if = "Option::is_none")]
    abort_incomplete_multipart_upload: Option<AbortIncompleteMultipartUpload>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct LifecycleFilter {
    #[serde(rename = "Prefix", skip_serializing_if = "Option::is_none")]
    prefix: Option<String>,
    #[serde(rename = "Tag", skip_serializing_if = "Option::is_none")]
    tag: Option<LifecycleTag>,
    #[serde(rename = "And", skip_serializing_if = "Option::is_none")]
    and: Option<LifecycleAnd>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct LifecycleAnd {
    #[serde(rename = "Prefix", skip_serializing_if = "Option::is_none")]
    prefix: Option<String>,
    #[serde(rename = "Tags", skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<LifecycleTag>>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct LifecycleTag {
    #[serde(rename = "Key")]
    key: String,
    #[serde(rename = "Value")]
    value: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct LifecycleTransition {
    #[serde(rename = "Days", skip_serializing_if = "Option::is_none")]
    days: Option<u32>,
    #[serde(rename = "Date", skip_serializing_if = "Option::is_none")]
    date: Option<String>,
    #[serde(rename = "StorageClass")]
    storage_class: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct LifecycleExpiration {
    #[serde(rename = "Days", skip_serializing_if = "Option::is_none")]
    days: Option<u32>,
    #[serde(rename = "Date", skip_serializing_if = "Option::is_none")]
    date: Option<String>,
    #[serde(rename = "ExpiredObjectDeleteMarker", skip_serializing_if = "Option::is_none")]
    expired_object_delete_marker: Option<bool>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct NoncurrentVersionTransition {
    #[serde(rename = "NoncurrentDays")]
    noncurrent_days: u32,
    #[serde(rename = "StorageClass")]
    storage_class: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct NoncurrentVersionExpiration {
    #[serde(rename = "NoncurrentDays")]
    noncurrent_days: u32,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct AbortIncompleteMultipartUpload {
    #[serde(rename = "DaysAfterInitiation")]
    days_after_initiation: u32,
}

#[derive(Clone)]
struct ObjectData {
    data: Vec<u8>,
    etag: String,
    last_modified: chrono::DateTime<Utc>,
    size: usize,
}

#[derive(Clone, Serialize, Deserialize)]
struct ObjectVersion {
    version_id: String,
    data: Vec<u8>,
    etag: String,
    last_modified: chrono::DateTime<Utc>,
    size: usize,
    is_latest: bool,
    is_delete_marker: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct ObjectMetadata {
    key: String,
    size: u64,
    etag: String,
    last_modified: DateTime<Utc>,
    content_type: String,
    storage_class: String,
    metadata: HashMap<String, String>,
    version_id: Option<String>,
    encryption: Option<ObjectEncryption>,
}

#[derive(Clone, Serialize, Deserialize)]
struct ObjectEncryption {
    algorithm: String,
    key_base64: String, // Base64 encoded encryption key
    nonce_base64: String, // Base64 encoded nonce for GCM
}

#[derive(Clone)]
struct MultipartUpload {
    upload_id: String,
    bucket: String,
    key: String,
    parts: HashMap<i32, UploadPart>,
    initiated: chrono::DateTime<Utc>,
}

#[derive(Clone)]
struct UploadPart {
    part_number: i32,
    etag: String,
    size: usize,
    data: Vec<u8>,
}

// Function to recursively remove empty directories
async fn cleanup_empty_directories(storage_path: PathBuf) {
    let auto_remove = env::var("AUTO_REMOVE_EMPTY_FOLDERS")
        .unwrap_or_else(|_| "0".to_string());

    if auto_remove != "1" {
        info!("Auto-remove empty folders is disabled");
        return;
    }

    let interval_minutes = env::var("AUTO_REMOVE_EMPTY_FOLDERS_EVERY_X_MIN")
        .unwrap_or_else(|_| "5".to_string())
        .parse::<u64>()
        .unwrap_or(5);

    info!("Starting empty folder cleanup task - will run every {} minutes", interval_minutes);

    let mut interval = tokio::time::interval(Duration::from_secs(interval_minutes * 60));

    loop {
        interval.tick().await;

        info!("Running empty folder cleanup scan...");
        let mut removed_count = 0;

        // Scan all bucket directories
        if let Ok(entries) = fs::read_dir(&storage_path) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    // This is a bucket directory - never delete it, only clean inside
                    removed_count += remove_empty_dirs_in_bucket(&entry.path());
                }
            }
        }

        if removed_count > 0 {
            info!("Cleanup completed: removed {} empty directories", removed_count);
        } else {
            debug!("Cleanup completed: no empty directories found");
        }
    }
}

// Helper function to remove empty subdirectories within a bucket (never the bucket itself)
fn remove_empty_dirs_in_bucket(bucket_dir: &std::path::Path) -> usize {
    let mut removed_count = 0;

    if let Ok(entries) = fs::read_dir(bucket_dir) {
        let subdirs: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();

        // Process each subdirectory
        for subdir in &subdirs {
            removed_count += remove_empty_subdir_recursive(&subdir.path());
        }
    }

    removed_count
}

// Recursively remove empty subdirectories (used for directories inside buckets)
fn remove_empty_subdir_recursive(dir: &std::path::Path) -> usize {
    let mut removed_count = 0;

    // First, recursively process all subdirectories
    if let Ok(entries) = fs::read_dir(dir) {
        let subdirs: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();

        // Recursively clean subdirectories first
        for subdir in &subdirs {
            removed_count += remove_empty_subdir_recursive(&subdir.path());
        }
    }

    // Now check if this directory is empty and can be removed
    // Don't remove .multipart directories as they may be needed
    if dir.file_name() != Some(std::ffi::OsStr::new(".multipart")) {
        if let Ok(mut entries) = fs::read_dir(dir) {
            if entries.next().is_none() {
                // Directory is empty
                if fs::remove_dir(dir).is_ok() {
                    debug!("Removed empty directory: {:?}", dir);
                    removed_count += 1;
                }
            }
        }
    }

    removed_count
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ironbucket=debug,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting IronBucket S3-compatible server with full API support...");

    // Get storage path from environment variable or use default
    let storage_path = std::env::var("STORAGE_PATH")
        .unwrap_or_else(|_| "/s3".to_string());
    let storage_path = PathBuf::from(storage_path);
    fs::create_dir_all(&storage_path).unwrap();
    info!("Using storage path: {:?}", storage_path);

    // Load credentials from environment variables (required)
    let access_key = std::env::var("ACCESS_KEY")
        .expect("ACCESS_KEY environment variable must be set");
    let secret_key = std::env::var("SECRET_KEY")
        .expect("SECRET_KEY environment variable must be set");

    let mut access_keys = HashMap::new();
    access_keys.insert(access_key.clone(), secret_key.clone());

    info!("Using access key: {}", access_key);

    let state = AppState {
        storage_path: storage_path.clone(),
        buckets: Arc::new(Mutex::new(HashMap::new())),
        access_keys: Arc::new(access_keys),
        multipart_uploads: Arc::new(Mutex::new(HashMap::new())),
    };

    let app = Router::new()
        // Root endpoints
        .route("/", get(list_buckets))
        .route("/", post(handle_root_post))

        // Bucket endpoints with query parameter support
        .route("/:bucket", get(handle_bucket_get))
        .route("/:bucket", put(handle_bucket_put))
        .route("/:bucket", post(handle_bucket_post))
        .route("/:bucket", delete(delete_bucket))
        .route("/:bucket", head(head_bucket))
        .route("/:bucket/", get(handle_bucket_get))
        .route("/:bucket/", put(handle_bucket_put))
        .route("/:bucket/", post(handle_bucket_post))
        .route("/:bucket/", delete(delete_bucket))
        .route("/:bucket/", head(head_bucket))

        // Object endpoints with query parameter support
        .route("/:bucket/*key", get(handle_object_get))
        .route("/:bucket/*key", put(handle_object_put))
        .route("/:bucket/*key", post(handle_object_post))
        .route("/:bucket/*key", delete(handle_object_delete))
        .route("/:bucket/*key", head(head_object))

        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .layer(CorsLayer::permissive())
        .layer(DefaultBodyLimit::disable()) // Disable body limit for S3 compatibility
        .with_state(state);

    // Spawn the background cleanup task
    tokio::spawn(cleanup_empty_directories(storage_path.clone()));

    let addr = SocketAddr::from(([0, 0, 0, 0], 9000));
    info!("IronBucket listening on {} with full S3 API support", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// Root POST handler
async fn handle_root_post(
    State(_state): State<AppState>,
    _headers: HeaderMap,
    _body: Bytes,
) -> impl IntoResponse {
    debug!("Handling POST / - form-based upload");
    // For now, return method not implemented
    // This would handle browser-based form uploads
    Response::builder()
        .status(StatusCode::NOT_IMPLEMENTED)
        .body(Body::from("Form-based uploads not yet implemented"))
        .unwrap()
}

// Query parameters for bucket operations
#[derive(Deserialize, Debug)]
struct BucketQueryParams {
    location: Option<String>,
    versioning: Option<String>,
    versions: Option<String>,
    acl: Option<String>,
    policy: Option<String>,
    encryption: Option<String>,
    cors: Option<String>,
    lifecycle: Option<String>,
    uploads: Option<String>,
    delete: Option<String>,
    #[serde(rename = "max-keys")]
    max_keys: Option<usize>,
    prefix: Option<String>,
    #[serde(rename = "continuation-token")]
    continuation_token: Option<String>,
    delimiter: Option<String>,
    #[serde(rename = "list-type")]
    list_type: Option<String>,
}

// Handle bucket GET with query parameters
async fn handle_bucket_get(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Query(params): Query<BucketQueryParams>,
) -> impl IntoResponse {
    debug!("GET bucket: {} with params: {:?}", bucket, params);

    // Check if bucket exists
    {
        let buckets = state.buckets.lock().unwrap();
        if !buckets.contains_key(&bucket) {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap();
        }
    }

    // Handle different query parameters
    if params.location.is_some() {
        // Return bucket location
        let location_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<LocationConstraint xmlns="http://s3.amazonaws.com/doc/2006-03-01/">us-east-1</LocationConstraint>"#;
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/xml")
            .body(Body::from(location_xml))
            .unwrap();
    }

    if params.versioning.is_some() {
        // Return versioning status
        let buckets = state.buckets.lock().unwrap();
        let status = if let Some(bucket_data) = buckets.get(&bucket) {
            bucket_data.versioning_status.clone()
        } else {
            // Try to load from disk if not in memory
            let versioning_file = state.storage_path.join(&bucket).join(".versioning");
            if versioning_file.exists() {
                fs::read_to_string(&versioning_file).ok()
            } else {
                None
            }
        };

        let versioning_xml = if let Some(status) = status {
            format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<VersioningConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
    <Status>{}</Status>
</VersioningConfiguration>"#, status)
        } else {
            // AWS returns empty body when versioning is not configured
            String::new()
        };

        if versioning_xml.is_empty() {
            // Return empty body for no versioning configuration
            return Response::builder()
                .status(StatusCode::OK)
                .body(Body::empty())
                .unwrap();
        } else {
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/xml")
                .body(Body::from(versioning_xml))
                .unwrap();
        }
    }

    if params.acl.is_some() {
        // Return bucket ACL
        let acl_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<AccessControlPolicy>
    <Owner>
        <ID>ironbucket</ID>
        <DisplayName>IronBucket</DisplayName>
    </Owner>
    <AccessControlList>
        <Grant>
            <Grantee xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="CanonicalUser">
                <ID>ironbucket</ID>
                <DisplayName>IronBucket</DisplayName>
            </Grantee>
            <Permission>FULL_CONTROL</Permission>
        </Grant>
    </AccessControlList>
</AccessControlPolicy>"#;
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/xml")
            .body(Body::from(acl_xml))
            .unwrap();
    }

    if params.policy.is_some() {
        // Return bucket policy
        let buckets = state.buckets.lock().unwrap();

        if let Some(bucket_data) = buckets.get(&bucket) {
            if let Some(ref policy) = bucket_data.policy {
                return Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(policy.clone()))
                    .unwrap();
            } else {
                // Try to load from disk if not in memory
                let policy_file = state.storage_path.join(&bucket).join(".policy");
                if policy_file.exists() {
                    if let Ok(policy) = fs::read_to_string(&policy_file) {
                        return Response::builder()
                            .status(StatusCode::OK)
                            .header(header::CONTENT_TYPE, "application/json")
                            .body(Body::from(policy))
                            .unwrap();
                    }
                }
            }
        }

        // No policy found
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "application/xml")
            .body(Body::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<Error>
    <Code>NoSuchBucketPolicy</Code>
    <Message>The bucket policy does not exist</Message>
</Error>"#))
            .unwrap();
    }

    if params.encryption.is_some() {
        // Return bucket encryption configuration
        let buckets = state.buckets.lock().unwrap();

        if let Some(bucket_data) = buckets.get(&bucket) {
            if let Some(ref encryption) = bucket_data.encryption {
                let encryption_xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<ServerSideEncryptionConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
    <Rule>
        <ApplyServerSideEncryptionByDefault>
            <SSEAlgorithm>{}</SSEAlgorithm>
            {}
        </ApplyServerSideEncryptionByDefault>
    </Rule>
</ServerSideEncryptionConfiguration>"#,
                    encryption.algorithm,
                    if let Some(ref kms_key) = encryption.kms_key_id {
                        format!("<KMSMasterKeyID>{}</KMSMasterKeyID>", kms_key)
                    } else {
                        String::new()
                    }
                );

                return Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/xml")
                    .body(Body::from(encryption_xml))
                    .unwrap();
            } else {
                // Try to load from disk if not in memory
                let encryption_file = state.storage_path.join(&bucket).join(".encryption");
                if encryption_file.exists() {
                    if let Ok(encryption_json) = fs::read_to_string(&encryption_file) {
                        if let Ok(encryption) = serde_json::from_str::<BucketEncryption>(&encryption_json) {
                            let encryption_xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<ServerSideEncryptionConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
    <Rule>
        <ApplyServerSideEncryptionByDefault>
            <SSEAlgorithm>{}</SSEAlgorithm>
            {}
        </ApplyServerSideEncryptionByDefault>
    </Rule>
</ServerSideEncryptionConfiguration>"#,
                                encryption.algorithm,
                                if let Some(ref kms_key) = encryption.kms_key_id {
                                    format!("<KMSMasterKeyID>{}</KMSMasterKeyID>", kms_key)
                                } else {
                                    String::new()
                                }
                            );

                            return Response::builder()
                                .status(StatusCode::OK)
                                .header(header::CONTENT_TYPE, "application/xml")
                                .body(Body::from(encryption_xml))
                                .unwrap();
                        }
                    }
                }
            }
        }

        // No encryption configuration
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "application/xml")
            .body(Body::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<Error>
    <Code>ServerSideEncryptionConfigurationNotFoundError</Code>
    <Message>The server side encryption configuration was not found</Message>
</Error>"#))
            .unwrap();
    }

    if params.cors.is_some() {
        // Return bucket CORS configuration in JSON format for AWS CLI
        let buckets = state.buckets.lock().unwrap();

        if let Some(bucket_data) = buckets.get(&bucket) {
            if let Some(ref cors) = bucket_data.cors {
                // Return CORS configuration as XML (AWS CLI will convert to JSON)
                let mut cors_xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<CORSConfiguration>"#);

                for rule in &cors.cors_rules {
                    cors_xml.push_str("\n  <CORSRule>");

                    if let Some(ref id) = rule.id {
                        cors_xml.push_str(&format!("\n    <ID>{}</ID>", id));
                    }

                    for origin in &rule.allowed_origins {
                        cors_xml.push_str(&format!("\n    <AllowedOrigin>{}</AllowedOrigin>", origin));
                    }

                    for method in &rule.allowed_methods {
                        cors_xml.push_str(&format!("\n    <AllowedMethod>{}</AllowedMethod>", method));
                    }

                    if let Some(ref headers) = rule.allowed_headers {
                        for header in headers {
                            cors_xml.push_str(&format!("\n    <AllowedHeader>{}</AllowedHeader>", header));
                        }
                    }

                    if let Some(ref headers) = rule.expose_headers {
                        for header in headers {
                            cors_xml.push_str(&format!("\n    <ExposeHeader>{}</ExposeHeader>", header));
                        }
                    }

                    if let Some(max_age) = rule.max_age_seconds {
                        cors_xml.push_str(&format!("\n    <MaxAgeSeconds>{}</MaxAgeSeconds>", max_age));
                    }

                    cors_xml.push_str("\n  </CORSRule>");
                }

                cors_xml.push_str("\n</CORSConfiguration>");

                return Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/xml")
                    .body(Body::from(cors_xml))
                    .unwrap();
            } else {
                // Try to load from disk if not in memory
                let cors_file = state.storage_path.join(&bucket).join(".cors");
                if cors_file.exists() {
                    if let Ok(cors_json) = fs::read_to_string(&cors_file) {
                        if let Ok(cors) = serde_json::from_str::<CorsConfiguration>(&cors_json) {
                            // Generate XML from loaded CORS config
                            let mut cors_xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<CORSConfiguration>"#);

                            for rule in &cors.cors_rules {
                                cors_xml.push_str("\n  <CORSRule>");

                                if let Some(ref id) = rule.id {
                                    cors_xml.push_str(&format!("\n    <ID>{}</ID>", id));
                                }

                                for origin in &rule.allowed_origins {
                                    cors_xml.push_str(&format!("\n    <AllowedOrigin>{}</AllowedOrigin>", origin));
                                }

                                for method in &rule.allowed_methods {
                                    cors_xml.push_str(&format!("\n    <AllowedMethod>{}</AllowedMethod>", method));
                                }

                                if let Some(ref headers) = rule.allowed_headers {
                                    for header in headers {
                                        cors_xml.push_str(&format!("\n    <AllowedHeader>{}</AllowedHeader>", header));
                                    }
                                }

                                if let Some(ref headers) = rule.expose_headers {
                                    for header in headers {
                                        cors_xml.push_str(&format!("\n    <ExposeHeader>{}</ExposeHeader>", header));
                                    }
                                }

                                if let Some(max_age) = rule.max_age_seconds {
                                    cors_xml.push_str(&format!("\n    <MaxAgeSeconds>{}</MaxAgeSeconds>", max_age));
                                }

                                cors_xml.push_str("\n  </CORSRule>");
                            }

                            cors_xml.push_str("\n</CORSConfiguration>");

                            return Response::builder()
                                .status(StatusCode::OK)
                                .header(header::CONTENT_TYPE, "application/xml")
                                .body(Body::from(cors_xml))
                                .unwrap();
                        }
                    }
                }
            }
        }

        // No CORS configuration
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "application/xml")
            .body(Body::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<Error>
    <Code>NoSuchCORSConfiguration</Code>
    <Message>The CORS configuration does not exist</Message>
</Error>"#))
            .unwrap();
    }

    if params.lifecycle.is_some() {
        // Return bucket lifecycle configuration
        let buckets = state.buckets.lock().unwrap();

        if let Some(bucket_data) = buckets.get(&bucket) {
            if let Some(ref lifecycle) = bucket_data.lifecycle {
                // Return lifecycle configuration as XML
                let mut lifecycle_xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<LifecycleConfiguration>"#);

                for rule in &lifecycle.rules {
                    lifecycle_xml.push_str("\n  <Rule>");

                    if let Some(ref id) = rule.id {
                        lifecycle_xml.push_str(&format!("\n    <ID>{}</ID>", id));
                    }

                    lifecycle_xml.push_str(&format!("\n    <Status>{}</Status>", rule.status));

                    // Add Filter if present
                    if let Some(ref filter) = rule.filter {
                        lifecycle_xml.push_str("\n    <Filter>");
                        if let Some(ref prefix) = filter.prefix {
                            lifecycle_xml.push_str(&format!("\n      <Prefix>{}</Prefix>", prefix));
                        }
                        if let Some(ref tag) = filter.tag {
                            lifecycle_xml.push_str(&format!("\n      <Tag>\n        <Key>{}</Key>\n        <Value>{}</Value>\n      </Tag>", tag.key, tag.value));
                        }
                        lifecycle_xml.push_str("\n    </Filter>");
                    }

                    // Add Expiration if present
                    if let Some(ref expiration) = rule.expiration {
                        lifecycle_xml.push_str("\n    <Expiration>");
                        if let Some(days) = expiration.days {
                            lifecycle_xml.push_str(&format!("\n      <Days>{}</Days>", days));
                        }
                        if let Some(ref date) = expiration.date {
                            lifecycle_xml.push_str(&format!("\n      <Date>{}</Date>", date));
                        }
                        lifecycle_xml.push_str("\n    </Expiration>");
                    }

                    // Add Transitions if present
                    if let Some(ref transitions) = rule.transitions {
                        for transition in transitions {
                            lifecycle_xml.push_str("\n    <Transition>");
                            if let Some(days) = transition.days {
                                lifecycle_xml.push_str(&format!("\n      <Days>{}</Days>", days));
                            }
                            if let Some(ref date) = transition.date {
                                lifecycle_xml.push_str(&format!("\n      <Date>{}</Date>", date));
                            }
                            lifecycle_xml.push_str(&format!("\n      <StorageClass>{}</StorageClass>", transition.storage_class));
                            lifecycle_xml.push_str("\n    </Transition>");
                        }
                    }

                    lifecycle_xml.push_str("\n  </Rule>");
                }

                lifecycle_xml.push_str("\n</LifecycleConfiguration>");

                return Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/xml")
                    .body(Body::from(lifecycle_xml))
                    .unwrap();
            } else {
                // Try to load from disk if not in memory
                let lifecycle_file = state.storage_path.join(&bucket).join(".lifecycle");
                if lifecycle_file.exists() {
                    if let Ok(lifecycle_json) = fs::read_to_string(&lifecycle_file) {
                        if let Ok(lifecycle) = serde_json::from_str::<LifecycleConfiguration>(&lifecycle_json) {
                            // Generate XML from loaded lifecycle config
                            let mut lifecycle_xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<LifecycleConfiguration>"#);

                            for rule in &lifecycle.rules {
                                lifecycle_xml.push_str("\n  <Rule>");

                                if let Some(ref id) = rule.id {
                                    lifecycle_xml.push_str(&format!("\n    <ID>{}</ID>", id));
                                }

                                lifecycle_xml.push_str(&format!("\n    <Status>{}</Status>", rule.status));

                                // Add Filter if present
                                if let Some(ref filter) = rule.filter {
                                    lifecycle_xml.push_str("\n    <Filter>");
                                    if let Some(ref prefix) = filter.prefix {
                                        lifecycle_xml.push_str(&format!("\n      <Prefix>{}</Prefix>", prefix));
                                    }
                                    if let Some(ref tag) = filter.tag {
                                        lifecycle_xml.push_str(&format!("\n      <Tag>\n        <Key>{}</Key>\n        <Value>{}</Value>\n      </Tag>", tag.key, tag.value));
                                    }
                                    lifecycle_xml.push_str("\n    </Filter>");
                                }

                                // Add Expiration if present
                                if let Some(ref expiration) = rule.expiration {
                                    lifecycle_xml.push_str("\n    <Expiration>");
                                    if let Some(days) = expiration.days {
                                        lifecycle_xml.push_str(&format!("\n      <Days>{}</Days>", days));
                                    }
                                    if let Some(ref date) = expiration.date {
                                        lifecycle_xml.push_str(&format!("\n      <Date>{}</Date>", date));
                                    }
                                    lifecycle_xml.push_str("\n    </Expiration>");
                                }

                                lifecycle_xml.push_str("\n  </Rule>");
                            }

                            lifecycle_xml.push_str("\n</LifecycleConfiguration>");

                            return Response::builder()
                                .status(StatusCode::OK)
                                .header(header::CONTENT_TYPE, "application/xml")
                                .body(Body::from(lifecycle_xml))
                                .unwrap();
                        }
                    }
                }
            }
        }

        // No lifecycle configuration found
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "application/xml")
            .body(Body::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<Error>
    <Code>NoSuchLifecycleConfiguration</Code>
    <Message>The lifecycle configuration does not exist</Message>
</Error>"#))
            .unwrap();
    }

    if params.uploads.is_some() {
        // List multipart uploads
        let uploads = state.multipart_uploads.lock().unwrap();
        let mut xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<ListMultipartUploadsResult>
    <Bucket>{}</Bucket>
    <MaxUploads>1000</MaxUploads>
    <IsTruncated>false</IsTruncated>"#, bucket);

        for (upload_id, upload) in uploads.iter() {
            if upload.bucket == bucket {
                xml.push_str(&format!(r#"
    <Upload>
        <Key>{}</Key>
        <UploadId>{}</UploadId>
        <Initiated>{}</Initiated>
    </Upload>"#, upload.key, upload_id, upload.initiated.to_rfc3339()));
            }
        }

        xml.push_str("\n</ListMultipartUploadsResult>");
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/xml")
            .body(Body::from(xml))
            .unwrap();
    }

    // List object versions
    if params.versions.is_some() {
        let buckets = state.buckets.lock().unwrap();
        let bucket_data = buckets.get(&bucket);

        let mut xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<ListVersionsResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
    <Name>{}</Name>
    <Prefix>{}</Prefix>
    <MaxKeys>{}</MaxKeys>
    <IsTruncated>false</IsTruncated>"#,
            bucket,
            params.prefix.as_deref().unwrap_or(""),
            params.max_keys.unwrap_or(1000)
        );

        if let Some(bucket_data) = bucket_data {
            let prefix = params.prefix.as_deref().unwrap_or("");
            let max_keys = params.max_keys.unwrap_or(1000);
            let mut count = 0;

            // List current objects as versions
            for (key, obj) in &bucket_data.objects {
                if count >= max_keys {
                    break;
                }
                if !prefix.is_empty() && !key.starts_with(prefix) {
                    continue;
                }

                // Load version ID from metadata file if available
                let metadata_path = state.storage_path.join(&bucket).join(format!("{}.metadata", key));
                let version_id = if metadata_path.exists() {
                    if let Ok(metadata_str) = fs::read_to_string(&metadata_path) {
                        if let Ok(metadata) = serde_json::from_str::<ObjectMetadata>(&metadata_str) {
                            metadata.version_id.unwrap_or_else(|| "null".to_string())
                        } else {
                            "null".to_string()
                        }
                    } else {
                        "null".to_string()
                    }
                } else {
                    "null".to_string()
                };

                xml.push_str(&format!(r#"
    <Version>
        <Key>{}</Key>
        <VersionId>{}</VersionId>
        <IsLatest>true</IsLatest>
        <LastModified>{}</LastModified>
        <ETag>"{}"</ETag>
        <Size>{}</Size>
        <StorageClass>STANDARD</StorageClass>
    </Version>"#,
                    key,
                    version_id,
                    obj.last_modified.to_rfc3339(),
                    obj.etag,
                    obj.size
                ));
                count += 1;
            }

            // List versioned objects
            for (key, versions) in &bucket_data.versions {
                if count >= max_keys {
                    break;
                }
                if !prefix.is_empty() && !key.starts_with(prefix) {
                    continue;
                }

                for version in versions {
                    if count >= max_keys {
                        break;
                    }

                    if version.is_delete_marker {
                        xml.push_str(&format!(r#"
    <DeleteMarker>
        <Key>{}</Key>
        <VersionId>{}</VersionId>
        <IsLatest>{}</IsLatest>
        <LastModified>{}</LastModified>
    </DeleteMarker>"#,
                            key,
                            version.version_id,
                            version.is_latest,
                            version.last_modified.to_rfc3339()
                        ));
                    } else {
                        xml.push_str(&format!(r#"
    <Version>
        <Key>{}</Key>
        <VersionId>{}</VersionId>
        <IsLatest>{}</IsLatest>
        <LastModified>{}</LastModified>
        <ETag>"{}"</ETag>
        <Size>{}</Size>
        <StorageClass>STANDARD</StorageClass>
    </Version>"#,
                            key,
                            version.version_id,
                            version.is_latest,
                            version.last_modified.to_rfc3339(),
                            version.etag,
                            version.size
                        ));
                    }
                    count += 1;
                }
            }
        }

        xml.push_str("\n</ListVersionsResult>");

        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/xml")
            .body(Body::from(xml))
            .unwrap();
    }

    // Default: list objects (handles both v1 and v2)
    // list-type=2 uses continuation-token, v1 uses marker
    info!("Handling list objects request for bucket: {}, list_type: {:?}", bucket, params.list_type);
    list_objects_impl(
        State(state),
        bucket,
        params.prefix,
        params.delimiter,
        params.continuation_token,
        params.max_keys
    ).await
}

// Handle bucket PUT with query parameters
async fn handle_bucket_put(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Query(params): Query<BucketQueryParams>,
    body: Bytes,
) -> impl IntoResponse {
    debug!("PUT bucket: {} with params: {:?}", bucket, params);

    if params.versioning.is_some() {
        // Parse versioning configuration from body
        let body_str = String::from_utf8_lossy(&body);
        debug!("Versioning configuration body: {}", body_str);

        // Extract status from XML body
        let status = if body_str.contains("<Status>Enabled</Status>") {
            Some("Enabled".to_string())
        } else if body_str.contains("<Status>Suspended</Status>") {
            Some("Suspended".to_string())
        } else {
            None
        };

        // Update bucket versioning status
        let mut buckets = state.buckets.lock().unwrap();
        if let Some(bucket_data) = buckets.get_mut(&bucket) {
            bucket_data.versioning_status = status.clone();
            info!("Set versioning status for bucket {} to {:?}", bucket, status);

            // Persist versioning status to disk
            let versioning_file = state.storage_path.join(&bucket).join(".versioning");
            if let Some(ref status) = status {
                if let Err(e) = fs::write(&versioning_file, status.as_bytes()) {
                    warn!("Failed to persist versioning status: {}", e);
                }
            }
        } else {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("NoSuchBucket"))
                .unwrap();
        }

        return Response::builder()
            .status(StatusCode::OK)
            .body(Body::empty())
            .unwrap();
    }

    if params.policy.is_some() {
        // Set bucket policy
        let policy_str = String::from_utf8_lossy(&body);
        debug!("Setting bucket policy: {}", policy_str);

        // Basic JSON validation
        if serde_json::from_str::<serde_json::Value>(&policy_str).is_err() {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header(header::CONTENT_TYPE, "application/xml")
                .body(Body::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<Error>
    <Code>MalformedPolicy</Code>
    <Message>The policy is not in the valid JSON format</Message>
</Error>"#))
                .unwrap();
        }

        // Update bucket policy
        let mut buckets = state.buckets.lock().unwrap();
        if let Some(bucket_data) = buckets.get_mut(&bucket) {
            bucket_data.policy = Some(policy_str.to_string());
            info!("Set policy for bucket {}", bucket);

            // Persist policy to disk
            let policy_file = state.storage_path.join(&bucket).join(".policy");
            if let Err(e) = fs::write(&policy_file, policy_str.as_bytes()) {
                warn!("Failed to persist bucket policy: {}", e);
            } else {
                debug!("Policy persisted to: {:?}", policy_file);
            }
        } else {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("NoSuchBucket"))
                .unwrap();
        }

        return Response::builder()
            .status(StatusCode::OK)
            .body(Body::empty())
            .unwrap();
    }

    if params.encryption.is_some() {
        // Set bucket encryption configuration
        let body_str = String::from_utf8_lossy(&body);
        debug!("Setting bucket encryption: {}", body_str);

        // Parse encryption configuration from XML
        let mut algorithm = "AES256".to_string();
        let mut kms_key_id = None;

        // Simple XML parsing
        if body_str.contains("<SSEAlgorithm>") {
            if let Some(start) = body_str.find("<SSEAlgorithm>") {
                if let Some(end) = body_str.find("</SSEAlgorithm>") {
                    algorithm = body_str[start + 14..end].to_string();
                }
            }
        }

        if body_str.contains("<KMSMasterKeyID>") {
            if let Some(start) = body_str.find("<KMSMasterKeyID>") {
                if let Some(end) = body_str.find("</KMSMasterKeyID>") {
                    kms_key_id = Some(body_str[start + 16..end].to_string());
                }
            }
        }

        // Validate algorithm
        if algorithm != "AES256" && algorithm != "aws:kms" {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header(header::CONTENT_TYPE, "application/xml")
                .body(Body::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<Error>
    <Code>InvalidEncryptionAlgorithm</Code>
    <Message>The encryption algorithm specified is not valid</Message>
</Error>"#))
                .unwrap();
        }

        // Update bucket encryption
        let mut buckets = state.buckets.lock().unwrap();
        if let Some(bucket_data) = buckets.get_mut(&bucket) {
            let encryption = BucketEncryption {
                algorithm: algorithm.clone(),
                kms_key_id: kms_key_id.clone(),
            };
            bucket_data.encryption = Some(encryption.clone());
            info!("Set encryption for bucket {}: {}", bucket, algorithm);

            // Persist encryption configuration to disk
            let encryption_file = state.storage_path.join(&bucket).join(".encryption");
            if let Ok(encryption_json) = serde_json::to_string(&encryption) {
                if let Err(e) = fs::write(&encryption_file, encryption_json) {
                    warn!("Failed to persist encryption configuration: {}", e);
                } else {
                    debug!("Encryption configuration persisted to: {:?}", encryption_file);
                }
            }
        } else {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("NoSuchBucket"))
                .unwrap();
        }

        return Response::builder()
            .status(StatusCode::OK)
            .body(Body::empty())
            .unwrap();
    }

    if params.cors.is_some() {
        // Parse CORS configuration from body (XML format from AWS CLI)
        let body_str = String::from_utf8_lossy(&body);
        debug!("CORS configuration body: {}", body_str);

        // Parse XML to extract CORS rules
        let mut cors_rules = Vec::new();

        // Split by CORSRule tags to parse each rule
        let rule_parts: Vec<&str> = body_str.split("<CORSRule>").collect();
        for (i, rule_part) in rule_parts.iter().enumerate() {
            if i == 0 { continue; } // Skip the part before first CORSRule

            let mut allowed_origins = Vec::new();
            let mut allowed_methods = Vec::new();
            let mut allowed_headers = None;
            let mut expose_headers = None;
            let mut max_age_seconds = None;
            let mut id = None;

            // Extract ID
            if let Some(id_start) = rule_part.find("<ID>") {
                if let Some(id_end) = rule_part.find("</ID>") {
                    id = Some(rule_part[id_start + 4..id_end].to_string());
                }
            }

            // Extract AllowedOrigins
            let origin_parts: Vec<&str> = rule_part.split("<AllowedOrigin>").collect();
            for (j, origin_part) in origin_parts.iter().enumerate() {
                if j == 0 { continue; }
                if let Some(end) = origin_part.find("</AllowedOrigin>") {
                    allowed_origins.push(origin_part[..end].to_string());
                }
            }

            // Extract AllowedMethods
            let method_parts: Vec<&str> = rule_part.split("<AllowedMethod>").collect();
            for (j, method_part) in method_parts.iter().enumerate() {
                if j == 0 { continue; }
                if let Some(end) = method_part.find("</AllowedMethod>") {
                    allowed_methods.push(method_part[..end].to_string());
                }
            }

            // Extract AllowedHeaders
            let header_parts: Vec<&str> = rule_part.split("<AllowedHeader>").collect();
            if header_parts.len() > 1 {
                let mut headers = Vec::new();
                for (j, header_part) in header_parts.iter().enumerate() {
                    if j == 0 { continue; }
                    if let Some(end) = header_part.find("</AllowedHeader>") {
                        headers.push(header_part[..end].to_string());
                    }
                }
                if !headers.is_empty() {
                    allowed_headers = Some(headers);
                }
            }

            // Extract ExposeHeaders
            let expose_parts: Vec<&str> = rule_part.split("<ExposeHeader>").collect();
            if expose_parts.len() > 1 {
                let mut headers = Vec::new();
                for (j, expose_part) in expose_parts.iter().enumerate() {
                    if j == 0 { continue; }
                    if let Some(end) = expose_part.find("</ExposeHeader>") {
                        headers.push(expose_part[..end].to_string());
                    }
                }
                if !headers.is_empty() {
                    expose_headers = Some(headers);
                }
            }

            // Extract MaxAgeSeconds
            if let Some(age_start) = rule_part.find("<MaxAgeSeconds>") {
                if let Some(age_end) = rule_part.find("</MaxAgeSeconds>") {
                    if let Ok(age) = rule_part[age_start + 15..age_end].parse::<u32>() {
                        max_age_seconds = Some(age);
                    }
                }
            }

            if !allowed_origins.is_empty() && !allowed_methods.is_empty() {
                cors_rules.push(CorsRule {
                    id,
                    allowed_origins,
                    allowed_methods,
                    allowed_headers,
                    expose_headers,
                    max_age_seconds,
                });
            }
        }

        if cors_rules.is_empty() {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header(header::CONTENT_TYPE, "application/xml")
                .body(Body::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<Error>
    <Code>MalformedXML</Code>
    <Message>The CORS configuration is not well-formed</Message>
</Error>"#))
                .unwrap();
        }

        // Store CORS configuration
        let mut buckets = state.buckets.lock().unwrap();
        if let Some(bucket_data) = buckets.get_mut(&bucket) {
            let cors_config = CorsConfiguration {
                cors_rules,
            };
            bucket_data.cors = Some(cors_config.clone());
            info!("Set CORS configuration for bucket {}", bucket);

            // Persist CORS configuration to disk
            let cors_file = state.storage_path.join(&bucket).join(".cors");
            if let Ok(cors_json) = serde_json::to_string(&cors_config) {
                if let Err(e) = fs::write(&cors_file, cors_json) {
                    warn!("Failed to persist CORS configuration: {}", e);
                } else {
                    debug!("CORS configuration persisted to: {:?}", cors_file);
                }
            }
        } else {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("NoSuchBucket"))
                .unwrap();
        }

        return Response::builder()
            .status(StatusCode::OK)
            .body(Body::empty())
            .unwrap();
    }

    if params.lifecycle.is_some() {
        // Parse lifecycle configuration from body (XML format from AWS CLI)
        let body_str = String::from_utf8_lossy(&body);
        debug!("Lifecycle configuration body: {}", body_str);

        // Parse XML to extract lifecycle rules
        let mut lifecycle_rules = Vec::new();

        // Split by Rule tags to parse each rule
        let rule_parts: Vec<&str> = body_str.split("<Rule>").collect();
        for (i, rule_part) in rule_parts.iter().enumerate() {
            if i == 0 { continue; } // Skip the part before first Rule

            let mut id = None;
            let mut status = String::from("Enabled");
            let mut filter = None;
            let mut expiration = None;
            let mut transitions = None;

            // Extract ID
            if let Some(id_start) = rule_part.find("<ID>") {
                if let Some(id_end) = rule_part.find("</ID>") {
                    id = Some(rule_part[id_start + 4..id_end].to_string());
                }
            }

            // Extract Status
            if let Some(status_start) = rule_part.find("<Status>") {
                if let Some(status_end) = rule_part.find("</Status>") {
                    status = rule_part[status_start + 8..status_end].to_string();
                }
            }

            // Extract Filter
            if let Some(filter_start) = rule_part.find("<Filter>") {
                if let Some(filter_end) = rule_part.find("</Filter>") {
                    let filter_xml = &rule_part[filter_start + 8..filter_end];
                    let mut prefix = None;
                    let mut tag = None;

                    // Extract Prefix
                    if let Some(prefix_start) = filter_xml.find("<Prefix>") {
                        if let Some(prefix_end) = filter_xml.find("</Prefix>") {
                            prefix = Some(filter_xml[prefix_start + 8..prefix_end].to_string());
                        }
                    }

                    // Extract Tag
                    if let Some(tag_start) = filter_xml.find("<Tag>") {
                        if let Some(tag_end) = filter_xml.find("</Tag>") {
                            let tag_xml = &filter_xml[tag_start + 5..tag_end];
                            let mut key = String::new();
                            let mut value = String::new();

                            if let Some(key_start) = tag_xml.find("<Key>") {
                                if let Some(key_end) = tag_xml.find("</Key>") {
                                    key = tag_xml[key_start + 5..key_end].to_string();
                                }
                            }
                            if let Some(value_start) = tag_xml.find("<Value>") {
                                if let Some(value_end) = tag_xml.find("</Value>") {
                                    value = tag_xml[value_start + 7..value_end].to_string();
                                }
                            }

                            if !key.is_empty() {
                                tag = Some(LifecycleTag { key, value });
                            }
                        }
                    }

                    if prefix.is_some() || tag.is_some() {
                        filter = Some(LifecycleFilter {
                            prefix,
                            tag,
                            and: None,
                        });
                    }
                }
            }

            // Extract Expiration
            if let Some(exp_start) = rule_part.find("<Expiration>") {
                if let Some(exp_end) = rule_part.find("</Expiration>") {
                    let exp_xml = &rule_part[exp_start + 12..exp_end];
                    let mut days = None;
                    let mut date = None;

                    if let Some(days_start) = exp_xml.find("<Days>") {
                        if let Some(days_end) = exp_xml.find("</Days>") {
                            if let Ok(d) = exp_xml[days_start + 6..days_end].parse::<u32>() {
                                days = Some(d);
                            }
                        }
                    }

                    if let Some(date_start) = exp_xml.find("<Date>") {
                        if let Some(date_end) = exp_xml.find("</Date>") {
                            date = Some(exp_xml[date_start + 6..date_end].to_string());
                        }
                    }

                    expiration = Some(LifecycleExpiration {
                        days,
                        date,
                        expired_object_delete_marker: None,
                    });
                }
            }

            // Extract Transitions
            let transition_parts: Vec<&str> = rule_part.split("<Transition>").collect();
            if transition_parts.len() > 1 {
                let mut trans_list = Vec::new();
                for (j, trans_part) in transition_parts.iter().enumerate() {
                    if j == 0 { continue; }
                    if let Some(end) = trans_part.find("</Transition>") {
                        let trans_xml = &trans_part[..end];
                        let mut days = None;
                        let mut date = None;
                        let mut storage_class = String::from("STANDARD_IA");

                        if let Some(days_start) = trans_xml.find("<Days>") {
                            if let Some(days_end) = trans_xml.find("</Days>") {
                                if let Ok(d) = trans_xml[days_start + 6..days_end].parse::<u32>() {
                                    days = Some(d);
                                }
                            }
                        }

                        if let Some(date_start) = trans_xml.find("<Date>") {
                            if let Some(date_end) = trans_xml.find("</Date>") {
                                date = Some(trans_xml[date_start + 6..date_end].to_string());
                            }
                        }

                        if let Some(sc_start) = trans_xml.find("<StorageClass>") {
                            if let Some(sc_end) = trans_xml.find("</StorageClass>") {
                                storage_class = trans_xml[sc_start + 14..sc_end].to_string();
                            }
                        }

                        trans_list.push(LifecycleTransition {
                            days,
                            date,
                            storage_class,
                        });
                    }
                }
                if !trans_list.is_empty() {
                    transitions = Some(trans_list);
                }
            }

            lifecycle_rules.push(LifecycleRule {
                id,
                status,
                filter,
                transitions,
                expiration,
                noncurrent_version_transitions: None,
                noncurrent_version_expiration: None,
                abort_incomplete_multipart_upload: None,
            });
        }

        let lifecycle_config = LifecycleConfiguration {
            rules: lifecycle_rules,
        };

        // Store lifecycle configuration
        let mut buckets = state.buckets.lock().unwrap();
        if let Some(bucket_data) = buckets.get_mut(&bucket) {
            bucket_data.lifecycle = Some(lifecycle_config.clone());
            info!("Set lifecycle configuration for bucket {}", bucket);

            // Persist to disk
            let lifecycle_file = state.storage_path.join(&bucket).join(".lifecycle");
            if let Ok(json) = serde_json::to_string_pretty(&lifecycle_config) {
                if let Err(e) = fs::write(&lifecycle_file, json) {
                    warn!("Failed to persist lifecycle configuration: {}", e);
                } else {
                    debug!("Lifecycle configuration persisted to {:?}", lifecycle_file);
                }
            }
        } else {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("NoSuchBucket"))
                .unwrap();
        }

        return Response::builder()
            .status(StatusCode::OK)
            .body(Body::empty())
            .unwrap();
    }

    if params.acl.is_some() {
        // Set ACL (just accept but don't actually implement)
        return Response::builder()
            .status(StatusCode::OK)
            .body(Body::empty())
            .unwrap();
    }

    // Default: create bucket
    create_bucket(State(state), Path(bucket)).await.into_response()
}

// Handle bucket POST with query parameters
async fn handle_bucket_post(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Query(params): Query<BucketQueryParams>,
    body: Bytes,
) -> impl IntoResponse {
    debug!("POST bucket: {} with params: {:?}", bucket, params);

    if params.delete.is_some() {
        // Parse batch delete request
        let body_str = String::from_utf8_lossy(&body);
        debug!("Batch delete request body: {}", body_str);

        // Structure to hold delete results
        #[derive(Debug)]
        struct DeleteObject {
            key: String,
            version_id: Option<String>,
        }

        #[derive(Debug)]
        struct DeleteResult {
            deleted: Vec<DeletedObject>,
            errors: Vec<DeleteError>,
        }

        #[derive(Debug)]
        struct DeletedObject {
            key: String,
            version_id: Option<String>,
            delete_marker: bool,
            delete_marker_version_id: Option<String>,
        }

        #[derive(Debug)]
        struct DeleteError {
            key: String,
            code: String,
            message: String,
            version_id: Option<String>,
        }

        let mut result = DeleteResult {
            deleted: Vec::new(),
            errors: Vec::new(),
        };

        // Parse XML to extract objects to delete
        // AWS sends compact XML on a single line, so we need to parse it differently
        let mut objects_to_delete = Vec::new();

        // Use regex to extract keys from the XML
        // Match <Key>...</Key> patterns
        let mut pos = 0;
        while let Some(key_start) = body_str[pos..].find("<Key>") {
            let key_start = pos + key_start + 5; // Skip past "<Key>"
            if let Some(key_end) = body_str[key_start..].find("</Key>") {
                let key = body_str[key_start..key_start + key_end].to_string();

                // Check if there's a version ID for this object
                let mut version_id = None;
                // Look for VersionId between this Key and the next </Object>
                if let Some(obj_end) = body_str[key_start..].find("</Object>") {
                    let obj_section = &body_str[key_start..key_start + obj_end];
                    if let Some(ver_start) = obj_section.find("<VersionId>") {
                        if let Some(ver_end) = obj_section[ver_start + 11..].find("</VersionId>") {
                            version_id = Some(obj_section[ver_start + 11..ver_start + 11 + ver_end].to_string());
                        }
                    }
                }

                objects_to_delete.push(DeleteObject {
                    key,
                    version_id,
                });

                pos = key_start + key_end;
            } else {
                break;
            }
        }

        // Process deletions
        for obj in objects_to_delete {
            let object_path = state.storage_path.join(&bucket).join(&obj.key);
            let metadata_path = state.storage_path.join(&bucket).join(format!("{}.metadata", obj.key));

            // Handle directory deletion attempts
            if obj.key.ends_with('/') || object_path.is_dir() {
                // In S3, deleting a "directory" (prefix) succeeds if it's empty
                if object_path.is_dir() {
                    // Try to remove empty directory
                    match fs::remove_dir(&object_path) {
                        Ok(_) => {
                            info!("Batch delete: Deleted empty directory: {}/{}", bucket, obj.key);
                            result.deleted.push(DeletedObject {
                                key: obj.key,
                                version_id: obj.version_id,
                                delete_marker: false,
                                delete_marker_version_id: None,
                            });
                        }
                        Err(_) => {
                            // Directory might not be empty or might not exist
                            // In S3, this is still considered successful
                            result.deleted.push(DeletedObject {
                                key: obj.key,
                                version_id: obj.version_id,
                                delete_marker: false,
                                delete_marker_version_id: None,
                            });
                        }
                    }
                } else {
                    // Path doesn't exist, but in S3 this is still successful
                    result.deleted.push(DeletedObject {
                        key: obj.key,
                        version_id: obj.version_id,
                        delete_marker: false,
                        delete_marker_version_id: None,
                    });
                }
                continue;
            }

            // Check if object exists
            if !object_path.exists() {
                result.errors.push(DeleteError {
                    key: obj.key.clone(),
                    code: "NoSuchKey".to_string(),
                    message: "The specified key does not exist.".to_string(),
                    version_id: obj.version_id,
                });
                continue;
            }

            // Delete from disk
            let mut delete_success = true;
            if let Err(e) = fs::remove_file(&object_path) {
                warn!("Failed to delete object file {}: {}", obj.key, e);
                delete_success = false;
            }

            // Delete metadata file if it exists
            if metadata_path.exists() {
                if let Err(e) = fs::remove_file(&metadata_path) {
                    warn!("Failed to delete metadata file for {}: {}", obj.key, e);
                }
            }

            // Delete from memory
            let mut buckets = state.buckets.lock().unwrap();
            if let Some(bucket_data) = buckets.get_mut(&bucket) {
                bucket_data.objects.remove(&obj.key);
            }

            if delete_success {
                result.deleted.push(DeletedObject {
                    key: obj.key,
                    version_id: obj.version_id,
                    delete_marker: false,
                    delete_marker_version_id: None,
                });
                info!("Batch delete: deleted object {}/{}", bucket, result.deleted.last().unwrap().key);
            } else {
                result.errors.push(DeleteError {
                    key: obj.key,
                    code: "InternalError".to_string(),
                    message: "We encountered an internal error. Please try again.".to_string(),
                    version_id: obj.version_id,
                });
            }
        }

        // Build response XML
        let mut xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<DeleteResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">"#);

        // Add successfully deleted objects
        for deleted in &result.deleted {
            xml.push_str(&format!(r#"
    <Deleted>
        <Key>{}</Key>"#, deleted.key));

            if let Some(ref version_id) = deleted.version_id {
                xml.push_str(&format!(r#"
        <VersionId>{}</VersionId>"#, version_id));
            }

            if deleted.delete_marker {
                xml.push_str(r#"
        <DeleteMarker>true</DeleteMarker>"#);
            }

            if let Some(ref marker_version_id) = deleted.delete_marker_version_id {
                xml.push_str(&format!(r#"
        <DeleteMarkerVersionId>{}</DeleteMarkerVersionId>"#, marker_version_id));
            }

            xml.push_str(r#"
    </Deleted>"#);
        }

        // Add errors
        for error in &result.errors {
            xml.push_str(&format!(r#"
    <Error>
        <Key>{}</Key>
        <Code>{}</Code>
        <Message>{}</Message>"#, error.key, error.code, error.message));

            if let Some(ref version_id) = error.version_id {
                xml.push_str(&format!(r#"
        <VersionId>{}</VersionId>"#, version_id));
            }

            xml.push_str(r#"
    </Error>"#);
        }

        xml.push_str("\n</DeleteResult>");

        info!("Batch delete completed: {} deleted, {} errors", result.deleted.len(), result.errors.len());

        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/xml")
            .body(Body::from(xml))
            .unwrap();
    }

    // Default: return method not allowed
    Response::builder()
        .status(StatusCode::METHOD_NOT_ALLOWED)
        .body(Body::empty())
        .unwrap()
}

// Query parameters for object operations
#[derive(Deserialize, Debug)]
struct ObjectQueryParams {
    uploads: Option<String>,
    #[serde(rename = "uploadId")]
    upload_id: Option<String>,
    #[serde(rename = "partNumber")]
    part_number: Option<i32>,
    acl: Option<String>,
}

// Handle object GET with query parameters
async fn handle_object_get(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(params): Query<ObjectQueryParams>,
) -> impl IntoResponse {
    debug!("GET object: {}/{} with params: {:?}", bucket, key, params);

    if params.acl.is_some() {
        // Return object ACL
        let acl_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<AccessControlPolicy>
    <Owner>
        <ID>ironbucket</ID>
        <DisplayName>IronBucket</DisplayName>
    </Owner>
    <AccessControlList>
        <Grant>
            <Grantee xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="CanonicalUser">
                <ID>ironbucket</ID>
                <DisplayName>IronBucket</DisplayName>
            </Grantee>
            <Permission>FULL_CONTROL</Permission>
        </Grant>
    </AccessControlList>
</AccessControlPolicy>"#;
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/xml")
            .body(Body::from(acl_xml))
            .unwrap();
    }

    if let Some(upload_id) = &params.upload_id {
        // List parts for multipart upload
        let uploads = state.multipart_uploads.lock().unwrap();
        if let Some(upload) = uploads.get(upload_id) {
            let mut xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<ListPartsResult>
    <Bucket>{}</Bucket>
    <Key>{}</Key>
    <UploadId>{}</UploadId>
    <MaxParts>1000</MaxParts>
    <IsTruncated>false</IsTruncated>"#, bucket, key, upload_id);

            let mut parts: Vec<_> = upload.parts.values().collect();
            parts.sort_by_key(|p| p.part_number);

            for part in parts {
                xml.push_str(&format!(r#"
    <Part>
        <PartNumber>{}</PartNumber>
        <ETag>"{}"</ETag>
        <Size>{}</Size>
    </Part>"#, part.part_number, part.etag, part.size));
            }

            xml.push_str("\n</ListPartsResult>");
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/xml")
                .body(Body::from(xml))
                .unwrap();
        }

        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap();
    }

    // Default: get object
    get_object(State(state), Path((bucket, key))).await.into_response()
}

// Handle object PUT with query parameters
async fn handle_object_put(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(params): Query<ObjectQueryParams>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    debug!("PUT object: {}/{} with params: {:?}", bucket, key, params);

    if params.acl.is_some() {
        // Set object ACL (just accept but don't actually implement)
        return Response::builder()
            .status(StatusCode::OK)
            .body(Body::empty())
            .unwrap();
    }

    if let (Some(upload_id), Some(part_number)) = (&params.upload_id, params.part_number) {
        // Upload part for multipart upload
        let data = body.to_vec();
        let etag = format!("{:x}", md5::compute(&data));

        let mut uploads = state.multipart_uploads.lock().unwrap();
        if let Some(upload) = uploads.get_mut(upload_id) {
            // Store part in memory
            upload.parts.insert(part_number, UploadPart {
                part_number,
                etag: etag.clone(),
                size: data.len(),
                data: data.clone(),
            });

            // Also persist part to disk
            let multipart_dir = state.storage_path.join(&upload.bucket).join(".multipart").join(upload_id);
            if let Err(e) = fs::create_dir_all(&multipart_dir) {
                warn!("Failed to create multipart parts directory: {}", e);
            }

            let part_path = multipart_dir.join(format!("part-{}", part_number));
            if let Err(e) = fs::write(&part_path, &data) {
                warn!("Failed to write part {} to disk: {}", part_number, e);
            }

            // Save part metadata
            let part_meta_path = multipart_dir.join(format!("part-{}.meta", part_number));
            let part_metadata = serde_json::json!({
                "part_number": part_number,
                "etag": etag,
                "size": data.len(),
            });

            if let Err(e) = fs::write(&part_meta_path, part_metadata.to_string()) {
                warn!("Failed to write part metadata: {}", e);
            } else {
                info!("Uploaded part {} for upload {}, size: {} bytes", part_number, upload_id, data.len());
            }

            return Response::builder()
                .status(StatusCode::OK)
                .header(header::ETAG, format!("\"{}\"", etag))
                .body(Body::empty())
                .unwrap();
        }

        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap();
    }

    // Default: put object
    put_object(State(state), Path((bucket, key)), headers, body).await.into_response()
}

// Handle object POST with query parameters
async fn handle_object_post(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(params): Query<ObjectQueryParams>,
    _body: Bytes,
) -> impl IntoResponse {
    debug!("POST object: {}/{} with params: {:?}", bucket, key, params);

    if params.uploads.is_some() {
        // Initiate multipart upload
        let upload_id = Uuid::new_v4().to_string();

        let initiated = Utc::now();
        let upload = MultipartUpload {
            upload_id: upload_id.clone(),
            bucket: bucket.clone(),
            key: key.clone(),
            parts: HashMap::new(),
            initiated,
        };

        state.multipart_uploads.lock().unwrap().insert(upload_id.clone(), upload);

        // Persist multipart upload metadata to disk
        let multipart_dir = state.storage_path.join(&bucket).join(".multipart");
        if let Err(e) = fs::create_dir_all(&multipart_dir) {
            warn!("Failed to create multipart directory: {}", e);
        }

        let upload_meta_path = multipart_dir.join(format!("{}.upload", upload_id));
        let upload_metadata = serde_json::json!({
            "upload_id": upload_id,
            "bucket": bucket,
            "key": key,
            "initiated": initiated.to_rfc3339(),
        });

        if let Err(e) = fs::write(&upload_meta_path, upload_metadata.to_string()) {
            warn!("Failed to write multipart upload metadata: {}", e);
        } else {
            info!("Initiated multipart upload: {} for {}/{}", upload_id, bucket, key);
        }

        let xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<InitiateMultipartUploadResult>
    <Bucket>{}</Bucket>
    <Key>{}</Key>
    <UploadId>{}</UploadId>
</InitiateMultipartUploadResult>"#, bucket, key, upload_id);

        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/xml")
            .body(Body::from(xml))
            .unwrap();
    }

    if let Some(upload_id) = &params.upload_id {
        // Complete multipart upload
        let mut uploads = state.multipart_uploads.lock().unwrap();
        if let Some(upload) = uploads.remove(upload_id) {
            // Combine all parts
            let mut combined_data = Vec::new();
            let mut parts: Vec<_> = upload.parts.into_iter().collect();
            parts.sort_by_key(|(num, _)| *num);

            for (_, part) in parts {
                combined_data.extend(part.data);
            }

            // Save the combined object
            let etag = format!("{:x}", md5::compute(&combined_data));

            // Create bucket directory if it doesn't exist
            let bucket_path = state.storage_path.join(&bucket);
            let _ = fs::create_dir_all(&bucket_path);

            // Write object to disk
            let object_path = bucket_path.join(&key);
            if let Some(parent) = object_path.parent() {
                let _ = fs::create_dir_all(parent);
            }

            if let Err(e) = fs::write(&object_path, &combined_data) {
                warn!("Failed to write multipart object: {}", e);
                return Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .unwrap();
            }

            // Save object metadata
            let metadata_path = state.storage_path.join(&bucket).join(format!("{}.metadata", key));
            let metadata = ObjectMetadata {
                key: key.clone(),
                size: combined_data.len() as u64,
                etag: etag.clone(),
                last_modified: Utc::now(),
                content_type: "binary/octet-stream".to_string(), // Default for multipart
                storage_class: "STANDARD".to_string(),
                metadata: HashMap::new(),
                version_id: None,
                encryption: None, // TODO: Add encryption support for multipart
            };

            if let Ok(metadata_json) = serde_json::to_string(&metadata) {
                if let Err(e) = fs::write(&metadata_path, metadata_json) {
                    warn!("Failed to write multipart object metadata: {}", e);
                } else {
                    info!("Multipart upload completed: {}/{}, size: {} bytes", bucket, key, combined_data.len());
                }
            }

            // Clean up multipart upload directory
            let multipart_dir = state.storage_path.join(&bucket).join(".multipart").join(upload_id);
            if let Err(e) = fs::remove_dir_all(&multipart_dir) {
                warn!("Failed to clean up multipart directory: {}", e);
            }

            // Update in-memory metadata
            let mut buckets = state.buckets.lock().unwrap();
            let bucket_data = buckets.entry(bucket.clone()).or_insert_with(|| BucketData {
                created: Utc::now(),
                objects: HashMap::new(),
                versioning_status: None,
                versions: HashMap::new(),
                policy: None,
                encryption: None,
                cors: None,
                lifecycle: None,
            });

            bucket_data.objects.insert(key.clone(), ObjectData {
                size: combined_data.len(),
                data: vec![],
                etag: etag.clone(),
                last_modified: Utc::now(),
            });

            let xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<CompleteMultipartUploadResult>
    <Location>http://s3.amazonaws.com/{}/{}</Location>
    <Bucket>{}</Bucket>
    <Key>{}</Key>
    <ETag>"{}"</ETag>
</CompleteMultipartUploadResult>"#, bucket, key, bucket, key, etag);

            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/xml")
                .body(Body::from(xml))
                .unwrap();
        }

        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap();
    }

    // Default: method not allowed
    Response::builder()
        .status(StatusCode::METHOD_NOT_ALLOWED)
        .body(Body::empty())
        .unwrap()
}

// Handle object DELETE with query parameters
async fn handle_object_delete(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(params): Query<ObjectQueryParams>,
) -> impl IntoResponse {
    debug!("DELETE object: {}/{} with params: {:?}", bucket, key, params);

    if let Some(upload_id) = &params.upload_id {
        // Abort multipart upload
        let mut uploads = state.multipart_uploads.lock().unwrap();
        if let Some(upload) = uploads.remove(upload_id) {
            // Clean up multipart upload directory and parts
            let multipart_dir = state.storage_path.join(&upload.bucket).join(".multipart").join(upload_id);
            if let Err(e) = fs::remove_dir_all(&multipart_dir) {
                warn!("Failed to clean up multipart directory during abort: {}", e);
            }

            // Remove upload metadata file
            let upload_meta_path = state.storage_path.join(&upload.bucket).join(".multipart").join(format!("{}.upload", upload_id));
            if let Err(e) = fs::remove_file(&upload_meta_path) {
                warn!("Failed to remove upload metadata file: {}", e);
            }

            info!("Aborted multipart upload: {}", upload_id);
            return StatusCode::NO_CONTENT.into_response();
        }
        return StatusCode::NOT_FOUND.into_response();
    }

    // Default: delete object
    delete_object(State(state), Path((bucket, key))).await.into_response()
}

async fn list_buckets(State(state): State<AppState>) -> impl IntoResponse {
    debug!("Listing buckets");
    let buckets = state.buckets.lock().unwrap();

    let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<ListAllMyBucketsResult>
    <Owner>
        <ID>ironbucket</ID>
        <DisplayName>IronBucket</DisplayName>
    </Owner>
    <Buckets>"#);

    for (name, data) in buckets.iter() {
        xml.push_str(&format!(
            r#"
        <Bucket>
            <Name>{}</Name>
            <CreationDate>{}</CreationDate>
        </Bucket>"#,
            name,
            data.created.to_rfc3339()
        ));
    }

    xml.push_str("\n    </Buckets>\n</ListAllMyBucketsResult>");

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/xml")
        .body(Body::from(xml))
        .unwrap()
}

async fn create_bucket(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> impl IntoResponse {
    info!("Creating bucket: {}", bucket);

    let mut buckets = state.buckets.lock().unwrap();
    if buckets.contains_key(&bucket) {
        // AWS S3 behavior: if bucket exists and owned by same user, return 200 OK
        debug!("Bucket {} already exists, returning OK (AWS S3 compatible behavior)", bucket);
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_LENGTH, "0")
            .header("x-amz-request-id", "ironbucket-request-id")
            .header("x-amz-id-2", "ironbucket-id-2")
            .body(Body::empty())
            .unwrap();
    }

    buckets.insert(
        bucket.clone(),
        BucketData {
            created: Utc::now(),
            objects: HashMap::new(),
            versioning_status: None,
            versions: HashMap::new(),
            policy: None,
            encryption: None,
            cors: None,
            lifecycle: None,
        },
    );

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_LENGTH, "0")
        .header("x-amz-request-id", "ironbucket-request-id")
        .header("x-amz-id-2", "ironbucket-id-2")
        .body(Body::empty())
        .unwrap()
}

async fn delete_bucket(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Query(params): Query<BucketQueryParams>,
) -> impl IntoResponse {
    info!("Deleting bucket: {} with params: {:?}", bucket, params);

    // Handle policy deletion
    if params.policy.is_some() {
        let mut buckets = state.buckets.lock().unwrap();
        if let Some(bucket_data) = buckets.get_mut(&bucket) {
            if bucket_data.policy.is_some() {
                bucket_data.policy = None;
                info!("Deleted policy for bucket {}", bucket);

                // Remove policy file from disk
                let policy_file = state.storage_path.join(&bucket).join(".policy");
                if policy_file.exists() {
                    if let Err(e) = fs::remove_file(&policy_file) {
                        warn!("Failed to delete policy file: {}", e);
                    } else {
                        debug!("Policy file deleted: {:?}", policy_file);
                    }
                }

                return Response::builder()
                    .status(StatusCode::NO_CONTENT)
                    .body(Body::empty())
                    .unwrap();
            } else {
                // No policy to delete
                return Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .header(header::CONTENT_TYPE, "application/xml")
                    .body(Body::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<Error>
    <Code>NoSuchBucketPolicy</Code>
    <Message>The bucket policy does not exist</Message>
</Error>"#))
                    .unwrap();
            }
        } else {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("NoSuchBucket"))
                .unwrap();
        }
    }

    // Handle encryption deletion
    if params.encryption.is_some() {
        let mut buckets = state.buckets.lock().unwrap();
        if let Some(bucket_data) = buckets.get_mut(&bucket) {
            if bucket_data.encryption.is_some() {
                bucket_data.encryption = None;
                info!("Deleted encryption configuration for bucket {}", bucket);

                // Remove encryption file from disk
                let encryption_file = state.storage_path.join(&bucket).join(".encryption");
                if encryption_file.exists() {
                    if let Err(e) = fs::remove_file(&encryption_file) {
                        warn!("Failed to delete encryption file: {}", e);
                    } else {
                        debug!("Encryption file deleted: {:?}", encryption_file);
                    }
                }

                return Response::builder()
                    .status(StatusCode::NO_CONTENT)
                    .body(Body::empty())
                    .unwrap();
            } else {
                // No encryption to delete
                return Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .header(header::CONTENT_TYPE, "application/xml")
                    .body(Body::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<Error>
    <Code>ServerSideEncryptionConfigurationNotFoundError</Code>
    <Message>The server side encryption configuration was not found</Message>
</Error>"#))
                    .unwrap();
            }
        } else {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("NoSuchBucket"))
                .unwrap();
        }
    }

    // Handle CORS deletion
    if params.cors.is_some() {
        let mut buckets = state.buckets.lock().unwrap();
        if let Some(bucket_data) = buckets.get_mut(&bucket) {
            if bucket_data.cors.is_some() {
                bucket_data.cors = None;
                info!("Deleted CORS configuration for bucket {}", bucket);

                // Remove CORS file from disk
                let cors_file = state.storage_path.join(&bucket).join(".cors");
                if cors_file.exists() {
                    if let Err(e) = fs::remove_file(&cors_file) {
                        warn!("Failed to delete CORS file: {}", e);
                    } else {
                        debug!("CORS file deleted: {:?}", cors_file);
                    }
                }

                return Response::builder()
                    .status(StatusCode::NO_CONTENT)
                    .body(Body::empty())
                    .unwrap();
            } else {
                // No CORS to delete
                return Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .header(header::CONTENT_TYPE, "application/xml")
                    .body(Body::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<Error>
    <Code>NoSuchCORSConfiguration</Code>
    <Message>The CORS configuration does not exist</Message>
</Error>"#))
                    .unwrap();
            }
        } else {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("NoSuchBucket"))
                .unwrap();
        }
    }

    // Handle lifecycle deletion
    if params.lifecycle.is_some() {
        let mut buckets = state.buckets.lock().unwrap();
        if let Some(bucket_data) = buckets.get_mut(&bucket) {
            if bucket_data.lifecycle.is_some() {
                bucket_data.lifecycle = None;
                info!("Deleted lifecycle configuration for bucket {}", bucket);

                // Remove lifecycle file from disk
                let lifecycle_file = state.storage_path.join(&bucket).join(".lifecycle");
                if lifecycle_file.exists() {
                    if let Err(e) = fs::remove_file(&lifecycle_file) {
                        warn!("Failed to delete lifecycle file: {}", e);
                    } else {
                        debug!("Lifecycle file deleted: {:?}", lifecycle_file);
                    }
                }

                return Response::builder()
                    .status(StatusCode::NO_CONTENT)
                    .body(Body::empty())
                    .unwrap();
            } else {
                // No lifecycle to delete
                return Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .header(header::CONTENT_TYPE, "application/xml")
                    .body(Body::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<Error>
    <Code>NoSuchLifecycleConfiguration</Code>
    <Message>The lifecycle configuration does not exist</Message>
</Error>"#))
                    .unwrap();
            }
        } else {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("NoSuchBucket"))
                .unwrap();
        }
    }

    // Default: delete the bucket itself
    let mut buckets = state.buckets.lock().unwrap();
    if buckets.remove(&bucket).is_some() {
        Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Body::empty())
            .unwrap()
    } else {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap()
    }
}

async fn head_bucket(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> impl IntoResponse {
    let buckets = state.buckets.lock().unwrap();
    if buckets.contains_key(&bucket) {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn list_objects_impl(
    state: State<AppState>,
    bucket: String,
    prefix: Option<String>,
    delimiter: Option<String>,
    continuation_token: Option<String>,
    max_keys: Option<usize>,
) -> Response {
    info!("Listing objects in bucket: {} with prefix: {:?}, delimiter: {:?}, continuation_token: {:?}, max_keys: {:?}",
           bucket, prefix, delimiter, continuation_token, max_keys);

    let buckets = state.buckets.lock().unwrap();
    let bucket_data = match buckets.get(&bucket) {
        Some(data) => data,
        None => return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap(),
    };

    let prefix_str = prefix.as_deref().unwrap_or("");
    let max_keys = max_keys.unwrap_or(1000);

    // Collect all matching objects into a sorted vector for consistent pagination
    let mut all_objects: Vec<_> = bucket_data.objects
        .iter()
        .filter(|(key, _)| key.starts_with(prefix_str))
        .collect();
    all_objects.sort_by_key(|(key, _)| key.as_str());

    // Apply pagination
    let start_after = continuation_token.as_deref().unwrap_or("");
    let start_index = if !start_after.is_empty() {
        // Find the index of the first object after the continuation token
        all_objects.iter().position(|(key, _)| key.as_str() > start_after).unwrap_or(all_objects.len())
    } else {
        0
    };

    // Get the requested page of objects
    let end_index = (start_index + max_keys).min(all_objects.len());
    let page_objects = &all_objects[start_index..end_index];

    // Check if there are more objects
    let is_truncated = end_index < all_objects.len();
    let next_continuation_token = if is_truncated {
        page_objects.last().map(|(key, _)| key.clone())
    } else {
        None
    };

    info!("Pagination debug: all_objects.len()={}, start_index={}, end_index={}, is_truncated={}, next_token={:?}",
           all_objects.len(), start_index, end_index, is_truncated, next_continuation_token);

    // Build common prefixes when delimiter is set
    let mut common_prefixes = Vec::new();
    if let Some(delim) = &delimiter {
        let mut seen_prefixes = std::collections::HashSet::new();
        for (key, _) in &all_objects {
            if let Some(idx) = key[prefix_str.len()..].find(delim) {
                let prefix_with_delim = format!("{}{}",
                    &key[..prefix_str.len() + idx], delim);
                if seen_prefixes.insert(prefix_with_delim.clone()) {
                    common_prefixes.push(prefix_with_delim);
                }
            }
        }
        common_prefixes.sort();
    }

    // Build XML response
    let mut xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult>
    <Name>{}</Name>
    <Prefix>{}</Prefix>"#,
        bucket,
        prefix_str
    );

    // Add delimiter if present
    if let Some(delim) = delimiter {
        xml.push_str(&format!("\n    <Delimiter>{}</Delimiter>", delim));
    }

    xml.push_str(&format!("\n    <MaxKeys>{}</MaxKeys>", max_keys));
    xml.push_str(&format!("\n    <IsTruncated>{}</IsTruncated>", is_truncated));

    // Add continuation token if present
    if let Some(token) = &continuation_token {
        xml.push_str(&format!("\n    <ContinuationToken>{}</ContinuationToken>", token));
    }

    // Add next continuation token if truncated
    if let Some(next_token) = &next_continuation_token {
        xml.push_str(&format!("\n    <NextContinuationToken>{}</NextContinuationToken>", next_token));
    }

    // Add common prefixes (folders) when delimiter is set
    for prefix in common_prefixes {
        xml.push_str(&format!(
            r#"
    <CommonPrefixes>
        <Prefix>{}</Prefix>
    </CommonPrefixes>"#,
            prefix
        ));
    }

    // Add objects
    for (key, obj) in page_objects {
        xml.push_str(&format!(
            r#"
    <Contents>
        <Key>{}</Key>
        <LastModified>{}</LastModified>
        <ETag>{}</ETag>
        <Size>{}</Size>
        <StorageClass>STANDARD</StorageClass>
    </Contents>"#,
            key,
            obj.last_modified.to_rfc3339(),
            obj.etag,
            obj.size
        ));
    }

    xml.push_str("\n</ListBucketResult>");

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/xml")
        .body(Body::from(xml))
        .unwrap()
}

async fn auth_middleware(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request<Body>,
    next: Next,
) -> Response {
    // Log all requests for debugging
    debug!("Request: {:?} {} {:?}", request.method(), request.uri(), headers);

    // OPTIONS requests bypass auth for CORS
    if request.method() == Method::OPTIONS {
        return next.run(request).await;
    }

    // Check for presigned URL authentication (query parameters)
    let uri = request.uri();
    if let Some(query) = uri.query() {
        // Check if this is a presigned URL with AWS Signature V4
        if query.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256") {
            // Parse query parameters
            let params: HashMap<String, String> = query
                .split('&')
                .filter_map(|param| {
                    let parts: Vec<&str> = param.split('=').collect();
                    if parts.len() == 2 {
                        Some((
                            parts[0].to_string(),
                            urlencoding::decode(parts[1]).unwrap_or_else(|_| parts[1].into()).to_string()
                        ))
                    } else {
                        None
                    }
                })
                .collect();

            // Extract access key from X-Amz-Credential
            if let Some(credential) = params.get("X-Amz-Credential") {
                // Credential format: ACCESS_KEY/20250915/us-east-1/s3/aws4_request
                let cred_parts: Vec<&str> = credential.split('/').collect();
                if !cred_parts.is_empty() {
                    let access_key = cred_parts[0];

                    // Check expiration
                    if let Some(expires_str) = params.get("X-Amz-Expires") {
                        if let Ok(expires_seconds) = expires_str.parse::<i64>() {
                            if let Some(date_str) = params.get("X-Amz-Date") {
                                // Parse date in format: 20250915T205242Z
                                if let Ok(request_time) = DateTime::parse_from_str(
                                    &format!(
                                        "{}-{}-{}T{}:{}:{}+00:00",
                                        &date_str[0..4],   // year
                                        &date_str[4..6],   // month
                                        &date_str[6..8],   // day
                                        &date_str[9..11],  // hour
                                        &date_str[11..13], // minute
                                        &date_str[13..15]  // second
                                    ),
                                    "%Y-%m-%dT%H:%M:%S%z"
                                ) {
                                    let now = Utc::now();
                                    let request_utc = request_time.with_timezone(&Utc);
                                    let elapsed = now.signed_duration_since(request_utc);

                                    // Check if URL has expired
                                    if elapsed.num_seconds() > expires_seconds {
                                        debug!("Presigned URL expired: {} seconds old, max {}", elapsed.num_seconds(), expires_seconds);
                                        return Response::builder()
                                            .status(StatusCode::FORBIDDEN)
                                            .body(Body::from("Request has expired"))
                                            .unwrap();
                                    }
                                }
                            }
                        }
                    }

                    // Check if access key exists
                    if state.access_keys.contains_key(access_key) {
                        // For presigned URLs, we should verify the signature
                        // For now, we'll do a simple check and accept valid access keys
                        // Full signature verification would require rebuilding the canonical request
                        debug!("Authenticated presigned URL request with access key: {}", access_key);
                        return next.run(request).await;
                    }
                }
            }
        }
    }

    // Check for AWS Signature V4 authentication in headers
    if let Some(auth_header) = headers.get("Authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if auth_str.starts_with("AWS4-HMAC-SHA256") {
                // Parse the authorization header
                let parts: Vec<&str> = auth_str.split(' ').collect();
                if parts.len() >= 2 {
                    let credential_part = parts[1];
                    if let Some(credential) = credential_part.strip_prefix("Credential=") {
                        let cred_parts: Vec<&str> = credential.split('/').collect();
                        if !cred_parts.is_empty() {
                            let access_key = cred_parts[0].split(',').next().unwrap_or("");

                            // Check if access key exists
                            if state.access_keys.contains_key(access_key) {
                                debug!("Authenticated request with access key: {}", access_key);
                                return next.run(request).await;
                            }
                        }
                    }
                }
            }
        }
    }

    // Return 403 Forbidden for unauthenticated requests
    debug!("Request without authentication, returning 403 Forbidden");
    Response::builder()
        .status(StatusCode::FORBIDDEN)
        .body(Body::from("Access Denied: Authentication required"))
        .unwrap()
}

async fn put_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    info!("Uploading object: {}/{}", bucket, key);

    // Check if this is chunked transfer encoding with signature
    let mut data = body.to_vec();

    // Check if the data starts with chunk size (hex) followed by ";chunk-signature="
    // Format: "3e8;chunk-signature=<64-char-hex>\r\n<data>\r\n0;chunk-signature=<64-char-hex>\r\n\r\n"
    if data.len() > 100 {
        let preview = String::from_utf8_lossy(&data[0..100]);
        if preview.contains(";chunk-signature=") {
            debug!("Detected chunked transfer encoding with signature, parsing chunks");
            data = parse_chunked_data(&data);
        }
    }
    let etag = format!("{:x}", md5::compute(&data));

    // Create bucket directory if it doesn't exist
    let bucket_path = state.storage_path.join(&bucket);
    if let Err(e) = fs::create_dir_all(&bucket_path) {
        warn!("Failed to create bucket directory: {}", e);
    }

    // Write object to disk
    let object_path = bucket_path.join(&key);
    if let Some(parent) = object_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            warn!("Failed to create object parent directory: {}", e);
        }
    }

    // Will be updated after encryption check
    // NOTE: We write the data first, then check encryption below
    // The actual write happens after we determine if encryption is needed

    // Check if versioning is enabled for this bucket
    let version_id = {
        let mut buckets = state.buckets.lock().unwrap();
        let bucket_data = buckets
            .entry(bucket.clone())
            .or_insert_with(|| BucketData {
                created: Utc::now(),
                objects: HashMap::new(),
                versioning_status: None,
                versions: HashMap::new(),
                policy: None,
                encryption: None,
                cors: None,
                lifecycle: None,
            });

        // Load versioning status from disk if not in memory
        if bucket_data.versioning_status.is_none() {
            let versioning_file = state.storage_path.join(&bucket).join(".versioning");
            if versioning_file.exists() {
                bucket_data.versioning_status = fs::read_to_string(&versioning_file).ok();
            }
        }

        let versioning_enabled = bucket_data.versioning_status.as_ref()
            .map(|s| s == "Enabled")
            .unwrap_or(false);

        if versioning_enabled {
            let vid = uuid::Uuid::new_v4().to_string();

            // Save versioned object to disk
            let versions_dir = bucket_path.join(".versions").join(&key);
            if let Err(e) = fs::create_dir_all(&versions_dir) {
                warn!("Failed to create versions directory: {}", e);
            }

            let version_path = versions_dir.join(&vid);
            if let Err(e) = fs::write(&version_path, &data) {
                warn!("Failed to write versioned object: {}", e);
            }

            // Mark all existing versions as not latest
            let versions = bucket_data.versions.entry(key.clone()).or_insert_with(Vec::new);
            for v in versions.iter_mut() {
                v.is_latest = false;
            }

            // Add new version
            versions.push(ObjectVersion {
                version_id: vid.clone(),
                data: data.clone(),
                etag: etag.clone(),
                last_modified: Utc::now(),
                size: data.len(),
                is_latest: true,
                is_delete_marker: false,
            });

            info!("Created version {} for object {}/{}", vid, bucket, key);
            Some(vid)
        } else {
            None
        }
    };

    // Save metadata to a separate file
    // Append .metadata to the full filename (including extension)
    let metadata_path = state.storage_path.join(&bucket).join(format!("{}.metadata", key));

    // Ensure parent directory exists for metadata file
    if let Some(parent) = metadata_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            warn!("Failed to create metadata parent directory: {}", e);
        }
    }

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    // Check if bucket has encryption enabled
    let encryption_info = {
        let buckets = state.buckets.lock().unwrap();
        if let Some(bucket_data) = buckets.get(&bucket) {
            if let Some(ref encryption) = bucket_data.encryption {
                if encryption.algorithm == "AES256" {
                    // Generate encryption key and encrypt data
                    let key = generate_encryption_key();
                    match encrypt_data(&data, &key) {
                        Ok((encrypted_data, nonce)) => {
                            Some((encrypted_data, ObjectEncryption {
                                algorithm: "AES256".to_string(),
                                key_base64: BASE64.encode(&key),
                                nonce_base64: BASE64.encode(&nonce),
                            }))
                        },
                        Err(e) => {
                            warn!("Failed to encrypt object: {}", e);
                            None
                        }
                    }
                } else {
                    None // KMS encryption not implemented
                }
            } else {
                None
            }
        } else {
            None
        }
    };

    // Use encrypted data if encryption is enabled
    let (final_data, object_encryption) = if let Some((encrypted_data, enc_info)) = encryption_info {
        (encrypted_data, Some(enc_info))
    } else {
        (data.clone(), None)
    };

    // Write the (possibly encrypted) data to disk
    if let Err(e) = fs::write(&object_path, &final_data) {
        warn!("Failed to write object to disk: {}", e);
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("Failed to store object"))
            .unwrap();
    }

    let metadata = ObjectMetadata {
        key: key.clone(),
        size: final_data.len() as u64,
        etag: etag.clone(),
        last_modified: Utc::now(),
        content_type,
        storage_class: "STANDARD".to_string(),
        metadata: HashMap::new(),
        version_id: version_id.clone(),
        encryption: object_encryption,
    };

    if let Ok(metadata_json) = serde_json::to_string(&metadata) {
        if let Err(e) = fs::write(&metadata_path, metadata_json) {
            warn!("Failed to write metadata file: {}", e);
        } else {
            debug!("Metadata saved to: {:?}", metadata_path);
        }
    }

    // Update in-memory metadata
    let mut buckets = state.buckets.lock().unwrap();
    let bucket_data = match buckets.get_mut(&bucket) {
        Some(data) => data,
        None => {
            // Create bucket if it doesn't exist
            buckets.insert(
                bucket.clone(),
                BucketData {
                    created: Utc::now(),
                    objects: HashMap::new(),
                    versioning_status: None,
                    versions: HashMap::new(),
                    policy: None,
                    encryption: None,
                    cors: None,
                    lifecycle: None,
                },
            );
            buckets.get_mut(&bucket).unwrap()
        }
    };

    bucket_data.objects.insert(
        key.clone(),
        ObjectData {
            size: data.len(),
            data: vec![], // Don't store data in memory, just metadata
            etag: etag.clone(),
            last_modified: Utc::now(),
        },
    );

    info!("Object stored at: {:?}", object_path);

    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(header::ETAG, format!("\"{}\"", etag));

    // Add version ID header if versioning is enabled
    if let Some(ref vid) = version_id {
        response = response.header("x-amz-version-id", vid);
    }

    response.body(Body::empty()).unwrap()
}

async fn get_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> impl IntoResponse {
    debug!("Getting object: {}/{}", bucket, key);

    // Read object from disk
    let object_path = state.storage_path.join(&bucket).join(&key);

    // First check if file exists on disk
    let data = match fs::read(&object_path) {
        Ok(data) => data,
        Err(_) => {
            // File doesn't exist on disk
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap();
        }
    };

    // Try to read metadata from file
    let metadata_path = state.storage_path.join(&bucket).join(format!("{}.metadata", key));
    let (data_to_return, etag, last_modified, content_type, encryption_header) = if let Ok(metadata_json) = fs::read_to_string(&metadata_path) {
        if let Ok(metadata) = serde_json::from_str::<ObjectMetadata>(&metadata_json) {
            // Check if object is encrypted and decrypt if necessary
            let (final_data, enc_header) = if let Some(encryption) = &metadata.encryption {
                if encryption.algorithm == "AES256" {
                    // Decode the key and nonce
                    let key = BASE64.decode(&encryption.key_base64).unwrap_or_default();
                    let nonce = BASE64.decode(&encryption.nonce_base64).unwrap_or_default();

                    // Decrypt the data
                    match decrypt_data(&data, &key, &nonce) {
                        Ok(decrypted) => (decrypted, Some("AES256".to_string())),
                        Err(e) => {
                            warn!("Failed to decrypt object: {}", e);
                            return Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(Body::from("Failed to decrypt object"))
                                .unwrap();
                        }
                    }
                } else {
                    (data.clone(), Some(encryption.algorithm.clone()))
                }
            } else {
                (data.clone(), None)
            };
            (final_data, metadata.etag, metadata.last_modified, metadata.content_type, enc_header)
        } else {
            // Metadata file exists but couldn't parse, fall back to defaults
            let etag = format!("{:x}", md5::compute(&data));
            (data.clone(), etag, Utc::now(), "application/octet-stream".to_string(), None)
        }
    } else {
        // No metadata file, check in-memory storage or calculate
        let buckets = state.buckets.lock().unwrap();
        let (etag, last_modified) = if let Some(bucket_data) = buckets.get(&bucket) {
            if let Some(obj) = bucket_data.objects.get(&key) {
                (obj.etag.clone(), obj.last_modified)
            } else {
                // File exists on disk but not in metadata, calculate etag
                let etag = format!("{:x}", md5::compute(&data));
                (etag, Utc::now())
            }
        } else {
            // File exists on disk but bucket not in metadata, calculate etag
            let etag = format!("{:x}", md5::compute(&data));
            (etag, Utc::now())
        };
        (data.clone(), etag, last_modified, "application/octet-stream".to_string(), None)
    };

    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, data_to_return.len().to_string())
        .header(header::ETAG, format!("\"{}\"", etag))
        .header(header::LAST_MODIFIED, format_http_date(&last_modified));

    // Add encryption header if object was encrypted
    if let Some(enc_algorithm) = encryption_header {
        response = response.header("x-amz-server-side-encryption", enc_algorithm);
    }

    response.body(Body::from(data_to_return)).unwrap()
}

async fn delete_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> impl IntoResponse {
    info!("Deleting object: {}/{}", bucket, key);

    // Check if the path is a directory
    let object_path = state.storage_path.join(&bucket).join(&key);

    // If path ends with '/' or is a directory, handle it as a prefix deletion
    if key.ends_with('/') || object_path.is_dir() {
        // In S3, deleting a "directory" (prefix) succeeds if it's empty
        // For filesystem-based storage, we try to remove the directory
        if object_path.is_dir() {
            // Try to remove empty directory
            match fs::remove_dir(&object_path) {
                Ok(_) => {
                    info!("Deleted empty directory: {}/{}", bucket, key);
                    return StatusCode::NO_CONTENT;
                }
                Err(e) => {
                    // Directory might not be empty or might not exist
                    debug!("Failed to delete directory {}/{}: {}", bucket, key, e);
                    // In S3, attempting to delete a non-existent prefix returns 204
                    return StatusCode::NO_CONTENT;
                }
            }
        } else {
            // Path doesn't exist, but in S3 this is still successful
            return StatusCode::NO_CONTENT;
        }
    }

    // Delete from disk
    let disk_deleted = fs::remove_file(&object_path).is_ok();

    // Also delete metadata file
    // Metadata is stored as filename.ext.metadata (not filename.metadata)
    let metadata_path = state.storage_path.join(&bucket).join(format!("{}.metadata", key));
    let metadata_deleted = fs::remove_file(&metadata_path).is_ok();

    if metadata_deleted {
        debug!("Deleted metadata file for {}/{}", bucket, key);
    }

    // Delete from in-memory metadata
    let mut buckets = state.buckets.lock().unwrap();
    let memory_deleted = if let Some(bucket_data) = buckets.get_mut(&bucket) {
        bucket_data.objects.remove(&key).is_some()
    } else {
        false
    };

    if disk_deleted || memory_deleted || metadata_deleted {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn head_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> impl IntoResponse {
    // Check if object exists on disk
    let object_path = state.storage_path.join(&bucket).join(&key);

    if !object_path.exists() {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap();
    }

    // Try to read metadata from file first
    let metadata_path = state.storage_path.join(&bucket).join(format!("{}.metadata", key));
    let (size, etag, last_modified, content_type) = if let Ok(metadata_json) = fs::read_to_string(&metadata_path) {
        if let Ok(metadata) = serde_json::from_str::<ObjectMetadata>(&metadata_json) {
            (metadata.size, metadata.etag, metadata.last_modified, metadata.content_type)
        } else {
            // Metadata file exists but couldn't parse, fall back to file stats
            let file_metadata = fs::metadata(&object_path).unwrap();
            let size = file_metadata.len();
            let data = fs::read(&object_path).unwrap_or_default();
            let etag = format!("{:x}", md5::compute(&data));
            (size, etag, Utc::now(), "application/octet-stream".to_string())
        }
    } else {
        // No metadata file, check in-memory storage or use file stats
        let buckets = state.buckets.lock().unwrap();
        if let Some(bucket_data) = buckets.get(&bucket) {
            if let Some(obj) = bucket_data.objects.get(&key) {
                return Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/octet-stream")
                    .header(header::CONTENT_LENGTH, obj.size.to_string())
                    .header(header::ETAG, format!("\"{}\"", obj.etag))
                    .header(header::LAST_MODIFIED, format_http_date(&obj.last_modified))
                    .body(Body::empty())
                    .unwrap();
            }
        }

        // Fall back to file stats
        let file_metadata = fs::metadata(&object_path).unwrap();
        let size = file_metadata.len();
        let data = fs::read(&object_path).unwrap_or_default();
        let etag = format!("{:x}", md5::compute(&data));
        (size, etag, Utc::now(), "application/octet-stream".to_string())
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, size.to_string())
        .header(header::ETAG, format!("\"{}\"", etag))
        .header(header::LAST_MODIFIED, format_http_date(&last_modified))
        .body(Body::empty())
        .unwrap()
}