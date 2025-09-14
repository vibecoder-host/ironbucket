use axum::{
    body::Body,
    extract::{Path, Query, State},
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
    fs,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tower_http::cors::CorsLayer;
use tracing::{info, debug, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use hmac::Hmac;
use sha2::Sha256;
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

#[derive(Clone)]
struct BucketData {
    created: chrono::DateTime<Utc>,
    objects: HashMap<String, ObjectData>,
    versioning_status: Option<String>, // "Enabled", "Suspended", or None
    versions: HashMap<String, Vec<ObjectVersion>>, // key -> list of versions
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

    // Initialize with default MinIO credentials
    let mut access_keys = HashMap::new();
    access_keys.insert("minioadmin".to_string(), "29d5bf40-b394-4923-bbf4-b1467964911d".to_string());

    let state = AppState {
        storage_path,
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
        .with_state(state);

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
    uploads: Option<String>,
    delete: Option<String>,
    #[serde(rename = "max-keys")]
    max_keys: Option<usize>,
    prefix: Option<String>,
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
            r#"<?xml version="1.0" encoding="UTF-8"?>
<VersioningConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
</VersioningConfiguration>"#.to_string()
        };

        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/xml")
            .body(Body::from(versioning_xml))
            .unwrap();
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

                xml.push_str(&format!(r#"
    <Version>
        <Key>{}</Key>
        <VersionId>null</VersionId>
        <IsLatest>true</IsLatest>
        <LastModified>{}</LastModified>
        <ETag>"{}"</ETag>
        <Size>{}</Size>
        <StorageClass>STANDARD</StorageClass>
    </Version>"#,
                    key,
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

    // Default: list objects
    list_objects_impl(State(state), bucket, params.prefix, params.max_keys).await
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
        let mut objects_to_delete = Vec::new();
        let mut current_key = None;
        let mut current_version_id = None;
        let mut in_object = false;

        for line in body_str.lines() {
            let line = line.trim();

            if line.contains("<Object>") {
                in_object = true;
                current_key = None;
                current_version_id = None;
            } else if line.contains("</Object>") {
                if let Some(key) = current_key.take() {
                    objects_to_delete.push(DeleteObject {
                        key,
                        version_id: current_version_id.take(),
                    });
                }
                in_object = false;
            } else if in_object {
                if line.contains("<Key>") && line.contains("</Key>") {
                    if let Some(start) = line.find("<Key>") {
                        if let Some(end) = line.find("</Key>") {
                            current_key = Some(line[start + 5..end].to_string());
                        }
                    }
                } else if line.contains("<VersionId>") && line.contains("</VersionId>") {
                    if let Some(start) = line.find("<VersionId>") {
                        if let Some(end) = line.find("</VersionId>") {
                            current_version_id = Some(line[start + 11..end].to_string());
                        }
                    }
                }
            }
        }

        // Process deletions
        for obj in objects_to_delete {
            let object_path = state.storage_path.join(&bucket).join(&obj.key);
            let metadata_path = object_path.with_extension("metadata");

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
            let metadata_path = object_path.with_extension("metadata");
            let metadata = ObjectMetadata {
                key: key.clone(),
                size: combined_data.len() as u64,
                etag: etag.clone(),
                last_modified: Utc::now(),
                content_type: "binary/octet-stream".to_string(), // Default for multipart
                storage_class: "STANDARD".to_string(),
                metadata: HashMap::new(),
                version_id: None,
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
) -> impl IntoResponse {
    info!("Deleting bucket: {}", bucket);

    let mut buckets = state.buckets.lock().unwrap();
    if buckets.remove(&bucket).is_some() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
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
    max_keys: Option<usize>,
) -> Response {
    debug!("Listing objects in bucket: {}", bucket);

    let buckets = state.buckets.lock().unwrap();
    let bucket_data = match buckets.get(&bucket) {
        Some(data) => data,
        None => return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap(),
    };

    let mut xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult>
    <Name>{}</Name>
    <Prefix>{}</Prefix>
    <MaxKeys>{}</MaxKeys>
    <IsTruncated>false</IsTruncated>"#,
        bucket,
        prefix.as_deref().unwrap_or(""),
        max_keys.unwrap_or(1000)
    );

    let prefix = prefix.as_deref().unwrap_or("");
    let max_keys = max_keys.unwrap_or(1000);
    let mut count = 0;

    for (key, obj) in &bucket_data.objects {
        if count >= max_keys {
            break;
        }
        if key.starts_with(prefix) {
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
            count += 1;
        }
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

    // Check for AWS Signature V4 authentication
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

    if let Err(e) = fs::write(&object_path, &data) {
        warn!("Failed to write object to disk: {}", e);
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("Failed to store object"))
            .unwrap();
    }

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
    let metadata_path = object_path.with_extension("metadata");
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let metadata = ObjectMetadata {
        key: key.clone(),
        size: data.len() as u64,
        etag: etag.clone(),
        last_modified: Utc::now(),
        content_type,
        storage_class: "STANDARD".to_string(),
        metadata: HashMap::new(),
        version_id: version_id.clone(),
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
    let metadata_path = object_path.with_extension("metadata");
    let (etag, last_modified, content_type) = if let Ok(metadata_json) = fs::read_to_string(&metadata_path) {
        if let Ok(metadata) = serde_json::from_str::<ObjectMetadata>(&metadata_json) {
            (metadata.etag, metadata.last_modified, metadata.content_type)
        } else {
            // Metadata file exists but couldn't parse, fall back to defaults
            let etag = format!("{:x}", md5::compute(&data));
            (etag, Utc::now(), "application/octet-stream".to_string())
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
        (etag, last_modified, "application/octet-stream".to_string())
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, data.len().to_string())
        .header(header::ETAG, format!("\"{}\"", etag))
        .header(header::LAST_MODIFIED, format_http_date(&last_modified))
        .body(Body::from(data))
        .unwrap()
}

async fn delete_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> impl IntoResponse {
    info!("Deleting object: {}/{}", bucket, key);

    // Delete from disk
    let object_path = state.storage_path.join(&bucket).join(&key);
    let disk_deleted = fs::remove_file(&object_path).is_ok();

    // Delete from in-memory metadata
    let mut buckets = state.buckets.lock().unwrap();
    let memory_deleted = if let Some(bucket_data) = buckets.get_mut(&bucket) {
        bucket_data.objects.remove(&key).is_some()
    } else {
        false
    };

    if disk_deleted || memory_deleted {
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
    let metadata_path = object_path.with_extension("metadata");
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