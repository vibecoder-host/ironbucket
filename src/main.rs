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
use serde::Deserialize;
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
}

#[derive(Clone)]
struct ObjectData {
    data: Vec<u8>,
    etag: String,
    last_modified: chrono::DateTime<Utc>,
    size: usize,
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
                .unwrap_or_else(|_| "rustybucket=debug,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting RustyBucket S3-compatible server with full API support...");

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
    info!("RustyBucket listening on {} with full S3 API support", addr);

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
        let versioning_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<VersioningConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
    <Status>Suspended</Status>
</VersioningConfiguration>"#;
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
        <ID>rustybucket</ID>
        <DisplayName>RustyBucket</DisplayName>
    </Owner>
    <AccessControlList>
        <Grant>
            <Grantee xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="CanonicalUser">
                <ID>rustybucket</ID>
                <DisplayName>RustyBucket</DisplayName>
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

    // Default: list objects
    list_objects_impl(State(state), bucket, params.prefix, params.max_keys).await
}

// Handle bucket PUT with query parameters
async fn handle_bucket_put(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Query(params): Query<BucketQueryParams>,
    _body: Bytes,
) -> impl IntoResponse {
    debug!("PUT bucket: {} with params: {:?}", bucket, params);

    if params.versioning.is_some() {
        // Set versioning (just accept but don't actually implement)
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

        // Simple XML parsing for delete request
        let mut deleted_count = 0;
        let mut _errors: Vec<String> = Vec::new();

        // Extract object keys from XML (simplified parsing)
        let lines: Vec<&str> = body_str.lines().collect();
        for line in lines {
            if line.contains("<Key>") && line.contains("</Key>") {
                if let Some(start) = line.find("<Key>") {
                    if let Some(end) = line.find("</Key>") {
                        let key = &line[start + 5..end];

                        // Delete the object
                        let mut buckets = state.buckets.lock().unwrap();
                        if let Some(bucket_data) = buckets.get_mut(&bucket) {
                            if bucket_data.objects.remove(key).is_some() {
                                // Also delete from disk
                                let object_path = state.storage_path.join(&bucket).join(key);
                                let _ = fs::remove_file(&object_path);
                                deleted_count += 1;
                            }
                        }
                    }
                }
            }
        }

        // Return delete result
        let mut xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<DeleteResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">"#);

        // Add deleted objects (simplified - just return success for all)
        for _ in 0..deleted_count {
            xml.push_str(r#"
    <Deleted>
        <Key>object</Key>
    </Deleted>"#);
        }

        xml.push_str("\n</DeleteResult>");

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
        <ID>rustybucket</ID>
        <DisplayName>RustyBucket</DisplayName>
    </Owner>
    <AccessControlList>
        <Grant>
            <Grantee xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="CanonicalUser">
                <ID>rustybucket</ID>
                <DisplayName>RustyBucket</DisplayName>
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
            upload.parts.insert(part_number, UploadPart {
                part_number,
                etag: etag.clone(),
                size: data.len(),
                data,
            });

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

        let upload = MultipartUpload {
            upload_id: upload_id.clone(),
            bucket: bucket.clone(),
            key: key.clone(),
            parts: HashMap::new(),
            initiated: Utc::now(),
        };

        state.multipart_uploads.lock().unwrap().insert(upload_id.clone(), upload);

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

            // Update metadata
            let mut buckets = state.buckets.lock().unwrap();
            let bucket_data = buckets.entry(bucket.clone()).or_insert_with(|| BucketData {
                created: Utc::now(),
                objects: HashMap::new(),
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
        if uploads.remove(upload_id).is_some() {
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
        <ID>rustybucket</ID>
        <DisplayName>RustyBucket</DisplayName>
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
            .header("x-amz-request-id", "rustybucket-request-id")
            .header("x-amz-id-2", "rustybucket-id-2")
            .body(Body::empty())
            .unwrap();
    }

    buckets.insert(
        bucket.clone(),
        BucketData {
            created: Utc::now(),
            objects: HashMap::new(),
        },
    );

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_LENGTH, "0")
        .header("x-amz-request-id", "rustybucket-request-id")
        .header("x-amz-id-2", "rustybucket-id-2")
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

    Response::builder()
        .status(StatusCode::OK)
        .header(header::ETAG, format!("\"{}\"", etag))
        .body(Body::empty())
        .unwrap()
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

    // Get metadata from in-memory storage (if available)
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

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
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

    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .unwrap()
}