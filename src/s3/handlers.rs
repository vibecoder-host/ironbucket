use crate::{
    error::{Error, Result},
    server::AppState,
    storage::{ObjectMetadata, StorageBackend},
};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::io::AsyncReadExt;
use tracing::{debug, info};

// Health check
pub async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

// List all buckets
pub async fn list_buckets(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse> {
    let buckets = state.storage.list_buckets().await?;
    let xml = super::xml::list_buckets_response(&buckets);

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/xml")],
        xml,
    ))
}

// Create bucket
pub async fn create_bucket(
    State(state): State<Arc<AppState>>,
    Path(bucket): Path<String>,
    _headers: HeaderMap,
    _body: Bytes,
) -> Result<impl IntoResponse> {
    info!("Creating bucket: {}", bucket);

    // Validate bucket name
    if !is_valid_bucket_name(&bucket) {
        return Err(Error::InvalidArgument("Invalid bucket name".to_string()));
    }

    state.storage.create_bucket(&bucket).await?;

    // Cache bucket creation
    if let Some(cluster) = &state.cluster {
        // Notify cluster nodes
        cluster.on_bucket_created(&bucket).await;
    }

    Ok((
        StatusCode::OK,
        [(header::LOCATION, format!("/{}", bucket))],
        "",
    ))
}

// Delete bucket
pub async fn delete_bucket(
    State(state): State<Arc<AppState>>,
    Path(bucket): Path<String>,
) -> Result<impl IntoResponse> {
    info!("Deleting bucket: {}", bucket);

    state.storage.delete_bucket(&bucket).await?;

    // Invalidate cache
    state.cache.invalidate_bucket(&bucket).await;

    // Notify cluster
    if let Some(cluster) = &state.cluster {
        cluster.on_bucket_deleted(&bucket).await;
    }

    Ok((StatusCode::NO_CONTENT, ""))
}

// Check if bucket exists
pub async fn head_bucket(
    State(state): State<Arc<AppState>>,
    Path(bucket): Path<String>,
) -> Result<impl IntoResponse> {
    if !state.storage.bucket_exists(&bucket).await? {
        return Err(Error::NoSuchBucket);
    }

    Ok((StatusCode::OK, ""))
}

// List objects in bucket
pub async fn list_objects(
    State(state): State<Arc<AppState>>,
    Path(bucket): Path<String>,
    Query(params): Query<ListObjectsParams>,
) -> Result<impl IntoResponse> {
    debug!("Listing objects in bucket: {} with params: {:?}", bucket, params);

    // Check from cache first
    let cache_key = format!("list:{}:{:?}", bucket, params);
    if let Some(cached) = state.cache.get(&cache_key).await {
        return Ok((
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/xml")],
            cached,
        ));
    }

    let result = state.storage.list_objects(
        &bucket,
        params.prefix.as_deref(),
        params.delimiter.as_deref(),
        params.continuation_token.as_deref(),
        params.max_keys.unwrap_or(1000),
    ).await?;

    let xml = super::xml::list_objects_response(&bucket, &result, &params);

    // Cache the result
    state.cache.set(&cache_key, xml.clone(), 60).await;

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/xml")],
        xml,
    ))
}

// Upload object
pub async fn put_object(
    State(state): State<Arc<AppState>>,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse> {
    info!("Uploading object: {}/{}", bucket, key);

    // Extract metadata from headers
    let mut metadata = HashMap::new();
    for (name, value) in headers.iter() {
        if name.as_str().starts_with("x-amz-meta-") {
            metadata.insert(
                name.as_str().trim_start_matches("x-amz-meta-").to_string(),
                value.to_str().unwrap_or("").to_string(),
            );
        }
    }

    // Add content type
    if let Some(content_type) = headers.get(header::CONTENT_TYPE) {
        metadata.insert(
            "content-type".to_string(),
            content_type.to_str().unwrap_or("application/octet-stream").to_string(),
        );
    }

    // Check if this is a multipart upload
    if let Some(_upload_id) = headers.get("x-amz-upload-id") {
        // Handle multipart upload
        return handle_multipart_upload(state, bucket, key, headers, body).await;
    }

    // Regular upload
    let data = body.to_vec();
    let object_metadata = state.storage.put_object(&bucket, &key, data, metadata).await?;

    // Invalidate cache
    state.cache.invalidate_object(&bucket, &key).await;

    // Notify cluster
    if let Some(cluster) = &state.cluster {
        cluster.on_object_created(&bucket, &key).await;
    }

    Ok((
        StatusCode::OK,
        [
            (header::ETAG, format!("\"{}\"", object_metadata.etag)),
            (header::LAST_MODIFIED, object_metadata.last_modified.format("%a, %d %b %Y %H:%M:%S GMT").to_string()),
        ],
        "",
    ))
}

// Download object
pub async fn get_object(
    State(state): State<Arc<AppState>>,
    Path((bucket, key)): Path<(String, String)>,
    _headers: HeaderMap,
) -> Result<impl IntoResponse> {
    debug!("Getting object: {}/{}", bucket, key);

    // Check cache for small objects
    let cache_key = format!("obj:{}:{}", bucket, key);
    if let Some(cached_data) = state.cache.get_bytes(&cache_key).await {
        if let Ok(metadata) = state.storage.head_object(&bucket, &key).await {
            return Ok(build_object_response(cached_data, &metadata));
        }
    }

    // Get object from storage
    let (mut stream, metadata) = state.storage.get_object_stream(&bucket, &key).await?;

    // For small files, read into memory and cache
    if metadata.size < 1024 * 1024 { // 1MB threshold
        let mut data = Vec::new();
        stream.read_to_end(&mut data).await?;

        // Cache small objects
        state.cache.set_bytes(&cache_key, data.clone(), 300).await;

        Ok(build_object_response(data, &metadata))
    } else {
        // Stream large files directly
        Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, &metadata.content_type)
            .header(header::CONTENT_LENGTH, metadata.size.to_string())
            .header(header::ETAG, format!("\"{}\"", metadata.etag))
            .header(header::LAST_MODIFIED, metadata.last_modified.to_rfc2822())
            .body(Body::from_stream(tokio_util::io::ReaderStream::new(stream)))
            .unwrap())
    }
}

// Delete object
pub async fn delete_object(
    State(state): State<Arc<AppState>>,
    Path((bucket, key)): Path<(String, String)>,
) -> Result<impl IntoResponse> {
    info!("Deleting object: {}/{}", bucket, key);

    state.storage.delete_object(&bucket, &key).await?;

    // Invalidate cache
    state.cache.invalidate_object(&bucket, &key).await;

    // Notify cluster
    if let Some(cluster) = &state.cluster {
        cluster.on_object_deleted(&bucket, &key).await;
    }

    Ok((StatusCode::NO_CONTENT, ""))
}

// Get object metadata
pub async fn head_object(
    State(state): State<Arc<AppState>>,
    Path((bucket, key)): Path<(String, String)>,
) -> Result<impl IntoResponse> {
    let metadata = state.storage.head_object(&bucket, &key).await?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, metadata.content_type),
            (header::CONTENT_LENGTH, metadata.size.to_string()),
            (header::ETAG, format!("\"{}\"", metadata.etag)),
            (header::LAST_MODIFIED, metadata.last_modified.to_rfc2822()),
        ],
        "",
    ))
}

// Handle bucket-level POST operations
pub async fn bucket_operations(
    State(state): State<Arc<AppState>>,
    Path(bucket): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    _headers: HeaderMap,
    _body: Bytes,
) -> Result<impl IntoResponse> {
    // Handle different operations based on query parameters
    if params.contains_key("uploads") {
        // Initiate multipart upload
        return initiate_multipart_upload(state, bucket, params, headers).await;
    }

    if params.contains_key("delete") {
        // Batch delete objects
        return batch_delete_objects(state, bucket, body).await;
    }

    Err(Error::NotImplemented)
}

// Handle object-level POST operations
pub async fn object_operations(
    State(state): State<Arc<AppState>>,
    Path((bucket, key)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
    _headers: HeaderMap,
    _body: Bytes,
) -> Result<impl IntoResponse> {
    if let Some(upload_id) = params.get("uploadId") {
        if params.contains_key("partNumber") {
            // Upload part
            return upload_part(state, bucket, key, upload_id.clone(), params, body).await;
        } else {
            // Complete multipart upload
            return complete_multipart_upload(state, bucket, key, upload_id.clone(), body).await;
        }
    }

    Err(Error::NotImplemented)
}

// Helper functions

fn is_valid_bucket_name(name: &str) -> bool {
    // S3 bucket naming rules
    name.len() >= 3
        && name.len() <= 63
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
        && !name.starts_with('.')
        && !name.ends_with('.')
        && !name.contains("..")
}

fn build_object_response(data: Vec<u8>, metadata: &ObjectMetadata) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, &metadata.content_type)
        .header(header::CONTENT_LENGTH, metadata.size.to_string())
        .header(header::ETAG, format!("\"{}\"", metadata.etag))
        .header(header::LAST_MODIFIED, metadata.last_modified.to_rfc2822())
        .body(Body::from(data))
        .unwrap()
}

// Multipart upload handlers (simplified stubs for now)

async fn handle_multipart_upload(
    _state: Arc<AppState>,
    _bucket: String,
    _key: String,
    _headers: HeaderMap,
    _body: Bytes,
) -> Result<impl IntoResponse> {
    // TODO: Implement multipart upload handling
    Err(Error::NotImplemented)
}

async fn initiate_multipart_upload(
    _state: Arc<AppState>,
    bucket: String,
    params: HashMap<String, String>,
    _headers: HeaderMap,
) -> Result<impl IntoResponse> {
    // TODO: Implement multipart upload initiation
    let upload_id = uuid::Uuid::new_v4().to_string();
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<InitiateMultipartUploadResult>
    <Bucket>{}</Bucket>
    <Key>{}</Key>
    <UploadId>{}</UploadId>
</InitiateMultipartUploadResult>"#,
        bucket,
        params.get("key").unwrap_or(&String::new()),
        upload_id
    );

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/xml")],
        xml,
    ))
}

async fn upload_part(
    _state: Arc<AppState>,
    _bucket: String,
    _key: String,
    _upload_id: String,
    _params: HashMap<String, String>,
    _body: Bytes,
) -> Result<impl IntoResponse> {
    // TODO: Implement part upload
    Err(Error::NotImplemented)
}

async fn complete_multipart_upload(
    _state: Arc<AppState>,
    _bucket: String,
    _key: String,
    _upload_id: String,
    _body: Bytes,
) -> Result<impl IntoResponse> {
    // TODO: Implement multipart upload completion
    Err(Error::NotImplemented)
}

async fn batch_delete_objects(
    _state: Arc<AppState>,
    _bucket: String,
    _body: Bytes,
) -> Result<impl IntoResponse> {
    // TODO: Implement batch delete
    Err(Error::NotImplemented)
}

// Query parameter structs

#[derive(Debug, Deserialize)]
pub struct ListObjectsParams {
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    #[serde(rename = "max-keys")]
    pub max_keys: Option<usize>,
    #[serde(rename = "continuation-token")]
    pub continuation_token: Option<String>,
    #[serde(rename = "list-type")]
    pub list_type: Option<u8>,
}