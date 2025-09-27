use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use chrono::{DateTime, Utc, TimeZone};
use std::{collections::HashMap, fs};
use tracing::{debug, info, warn};
use uuid::Uuid;
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce, Key,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use rand::RngCore;

use crate::{
    AppState, ObjectMetadata, ObjectEncryption,
    MultipartUpload, UploadPart, format_http_date,
    filesystem::{read_bucket_versioning, read_bucket_encryption},
    models::Operation, ObjectQueryParams,
};

// Use ObjectQueryParams from models

// Handle object GET with query parameters
pub async fn handle_object_get(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(params): Query<ObjectQueryParams>,
) -> impl IntoResponse {
    debug!("GET object: {}/{} with params: {:?}", bucket, key, params);

    // Increment stats for GET operation
    if let Err(e) = state.quota_manager.increment_stat(&bucket, Operation::Get).await {
        warn!("Failed to update GET stats for bucket {}: {}", bucket, e);
    }

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

    if params.tagging.is_some() {
        // Return object tags from metadata
        let metadata_path = state.storage_path.join(&bucket).join(format!("{}.metadata", key));

        let tags_xml = if metadata_path.exists() {
            // Read metadata file
            match fs::read_to_string(&metadata_path) {
                Ok(content) => {
                    match serde_json::from_str::<ObjectMetadata>(&content) {
                        Ok(metadata) => {
                            // Build XML from tags
                            let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?><Tagging><TagSet>"#);

                            if let Some(tags) = &metadata.tags {
                                for (key, value) in tags {
                                    xml.push_str(&format!("<Tag><Key>{}</Key><Value>{}</Value></Tag>", key, value));
                                }
                            }

                            xml.push_str("</TagSet></Tagging>");
                            xml
                        }
                        Err(e) => {
                            warn!("Failed to parse metadata file: {}", e);
                            // Return empty tags on parse error
                            r#"<?xml version="1.0" encoding="UTF-8"?><Tagging><TagSet/></Tagging>"#.to_string()
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to read metadata file: {}", e);
                    // Return empty tags if metadata doesn't exist
                    r#"<?xml version="1.0" encoding="UTF-8"?><Tagging><TagSet/></Tagging>"#.to_string()
                }
            }
        } else {
            // No metadata found, return empty tag set
            r#"<?xml version="1.0" encoding="UTF-8"?><Tagging><TagSet/></Tagging>"#.to_string()
        };

        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/xml")
            .body(Body::from(tags_xml))
            .unwrap();
    }

    // Handle versions query parameter - list all versions of an object
    if params.versions.is_some() {
        let versions_dir = state.storage_path.join(&bucket).join(".versions").join(&key);

        let mut xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<ListVersionsResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
    <Name>{}</Name>
    <Prefix>{}</Prefix>
    <KeyMarker></KeyMarker>
    <VersionIdMarker></VersionIdMarker>
    <MaxKeys>1000</MaxKeys>
    <IsTruncated>false</IsTruncated>"#, bucket, key);

        // Get current version info
        let object_path = state.storage_path.join(&bucket).join(&key);
        if object_path.exists() {
            let metadata = fs::metadata(&object_path).unwrap();
            let size = metadata.len();
            let last_modified = metadata.modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| Utc.timestamp_opt(d.as_secs() as i64, d.subsec_nanos()).unwrap())
                .unwrap_or_else(Utc::now);

            // Add current version (latest)
            xml.push_str(&format!(r#"
    <Version>
        <Key>{}</Key>
        <VersionId>null</VersionId>
        <IsLatest>true</IsLatest>
        <LastModified>{}</LastModified>
        <ETag>"{}"</ETag>
        <Size>{}</Size>
        <StorageClass>STANDARD</StorageClass>
        <Owner>
            <ID>ironbucket</ID>
            <DisplayName>IronBucket</DisplayName>
        </Owner>
    </Version>"#,
                key,
                last_modified.to_rfc3339(),
                format!("{:x}", md5::compute(fs::read(&object_path).unwrap_or_default())),
                size
            ));
        }

        // List versions from .versions directory
        if versions_dir.exists() && versions_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&versions_dir) {
                let mut versions: Vec<(String, DateTime<Utc>, u64)> = Vec::new();

                for entry in entries.flatten() {
                    if let Ok(metadata) = entry.metadata() {
                        if metadata.is_file() {
                            let file_name = entry.file_name().to_string_lossy().to_string();

                            // Skip metadata files
                            if file_name.ends_with(".metadata") {
                                continue;
                            }

                            let version_id = file_name.clone();

                            // Try to read last_modified from version metadata file
                            let version_metadata_path = versions_dir.join(format!("{}.metadata", &file_name));
                            let last_modified = if version_metadata_path.exists() {
                                if let Ok(metadata_str) = fs::read_to_string(&version_metadata_path) {
                                    if let Ok(obj_metadata) = serde_json::from_str::<ObjectMetadata>(&metadata_str) {
                                        obj_metadata.last_modified
                                    } else {
                                        // Fallback to file system time if metadata parsing fails
                                        metadata.modified()
                                            .ok()
                                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                            .map(|d| Utc.timestamp_opt(d.as_secs() as i64, d.subsec_nanos()).unwrap())
                                            .unwrap_or_else(Utc::now)
                                    }
                                } else {
                                    // Fallback to file system time if metadata file can't be read
                                    metadata.modified()
                                        .ok()
                                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                        .map(|d| Utc.timestamp_opt(d.as_secs() as i64, d.subsec_nanos()).unwrap())
                                        .unwrap_or_else(Utc::now)
                                }
                            } else {
                                // Use file system time if no metadata file exists
                                metadata.modified()
                                    .ok()
                                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                    .map(|d| Utc.timestamp_opt(d.as_secs() as i64, d.subsec_nanos()).unwrap())
                                    .unwrap_or_else(Utc::now)
                            };

                            let size = metadata.len();

                            versions.push((version_id, last_modified, size));
                        }
                    }
                }

                // Sort versions by date (newest first)
                versions.sort_by(|a, b| b.1.cmp(&a.1));

                // Add each version to XML
                for (version_id, last_modified, size) in versions {
                    let version_path = versions_dir.join(&version_id);
                    let etag = if let Ok(data) = fs::read(&version_path) {
                        format!("{:x}", md5::compute(&data))
                    } else {
                        "unknown".to_string()
                    };

                    xml.push_str(&format!(r#"
    <Version>
        <Key>{}</Key>
        <VersionId>{}</VersionId>
        <IsLatest>false</IsLatest>
        <LastModified>{}</LastModified>
        <ETag>"{}"</ETag>
        <Size>{}</Size>
        <StorageClass>STANDARD</StorageClass>
        <Owner>
            <ID>ironbucket</ID>
            <DisplayName>IronBucket</DisplayName>
        </Owner>
    </Version>"#,
                        key,
                        version_id,
                        last_modified.to_rfc3339(),
                        etag,
                        size
                    ));
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
    get_object(State(state), Path((bucket, key)), params.version_id).await.into_response()
}

// Handle object PUT with query parameters
pub async fn handle_object_put(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(params): Query<ObjectQueryParams>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    debug!("PUT object: {}/{} with params: {:?}", bucket, key, params);

    // Check quota before accepting upload (skip for ACL/tagging operations)
    if params.acl.is_none() && params.tagging.is_none() {
        let content_length = body.len() as u64;
        match state.quota_manager.check_quota(&bucket, content_length).await {
            Ok(false) => {
                warn!("Quota exceeded for bucket {}: attempted to add {} bytes", bucket, content_length);
                return Response::builder()
                    .status(StatusCode::INSUFFICIENT_STORAGE)
                    .header("x-amz-error-code", "QuotaExceeded")
                    .body(Body::from("Bucket quota exceeded"))
                    .unwrap();
            }
            Err(e) => {
                warn!("Failed to check quota for bucket {}: {}", bucket, e);
                // Continue anyway - don't fail on quota check errors
            }
            Ok(true) => {
                // Quota ok, continue
            }
        }
    }

    if params.acl.is_some() {
        // Set object ACL (just accept but don't actually implement)
        return Response::builder()
            .status(StatusCode::OK)
            .body(Body::empty())
            .unwrap();
    }

    if params.tagging.is_some() {
        // Handle object tagging
        info!("Setting tags for object: {}/{}", bucket, key);

        // Parse the XML body to extract tags
        let xml_str = String::from_utf8_lossy(&body);

        // Parse XML to extract tags into a HashMap
        let mut tags_map = HashMap::new();

        // Simple XML parsing for tags
        let tag_start = "<Tag>";
        let tag_end = "</Tag>";
        let key_start = "<Key>";
        let key_end = "</Key>";
        let value_start = "<Value>";
        let value_end = "</Value>";

        let mut pos = 0;
        while let Some(tag_pos) = xml_str[pos..].find(tag_start) {
            let tag_start_pos = pos + tag_pos + tag_start.len();
            if let Some(tag_end_pos) = xml_str[tag_start_pos..].find(tag_end) {
                let tag_content = &xml_str[tag_start_pos..tag_start_pos + tag_end_pos];

                // Extract key and value from tag content
                if let (Some(key_s), Some(key_e)) = (tag_content.find(key_start), tag_content.find(key_end)) {
                    if let (Some(val_s), Some(val_e)) = (tag_content.find(value_start), tag_content.find(value_end)) {
                        let key = &tag_content[key_s + key_start.len()..key_e];
                        let value = &tag_content[val_s + value_start.len()..val_e];
                        tags_map.insert(key.to_string(), value.to_string());
                    }
                }

                pos = tag_start_pos + tag_end_pos + tag_end.len();
            } else {
                break;
            }
        }

        // Read existing metadata
        let metadata_path = state.storage_path.join(&bucket).join(format!("{}.metadata", key));

        let metadata = if metadata_path.exists() {
            // Read existing metadata
            match fs::read_to_string(&metadata_path) {
                Ok(content) => {
                    match serde_json::from_str::<ObjectMetadata>(&content) {
                        Ok(mut md) => {
                            // Update tags
                            md.tags = if tags_map.is_empty() { None } else { Some(tags_map) };
                            md
                        }
                        Err(e) => {
                            warn!("Failed to parse metadata file: {}", e);
                            return Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(Body::from("Failed to parse metadata"))
                                .unwrap();
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to read metadata file: {}", e);
                    return Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Body::from("Object not found"))
                        .unwrap();
                }
            }
        } else {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("Object not found"))
                .unwrap();
        };

        // Write updated metadata
        if let Err(e) = fs::write(&metadata_path, serde_json::to_string(&metadata).unwrap()) {
            warn!("Failed to write metadata file: {}", e);
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("Failed to save tags"))
                .unwrap();
        }

        info!("Tags saved successfully for {}/{}", bucket, key);

        // Return success without modifying the actual object
        return Response::builder()
            .status(StatusCode::OK)
            .body(Body::empty())
            .unwrap();
    }

    if let (Some(upload_id), Some(part_number)) = (&params.upload_id, params.part_number) {
        // Upload part for multipart upload
        let mut data = body.to_vec();

        // Check if this is chunked transfer encoding with signature
        if data.len() > 100 {
            let preview = String::from_utf8_lossy(&data[0..100]);
            if preview.contains(";chunk-signature=") {
                debug!("Detected chunked transfer encoding in multipart upload part, parsing chunks");
                data = parse_chunked_data(&data);
            }
        }

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
pub async fn handle_object_post(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(params): Query<ObjectQueryParams>,
    body: Bytes,
) -> impl IntoResponse {
    debug!("POST object: {}/{} with params: {:?}", bucket, key, params);

    if params.uploads.is_some() {
        // Initiate multipart upload
        let upload_id = Uuid::new_v4().to_string();

        // Default Content-Type for multipart upload
        let content_type = "application/octet-stream".to_string();

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
            "content_type": content_type,
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
            // Read the stored content type from upload metadata
            let multipart_dir = state.storage_path.join(&bucket).join(".multipart");
            let upload_meta_path = multipart_dir.join(format!("{}.upload", upload_id));

            let stored_content_type = if let Ok(metadata_str) = fs::read_to_string(&upload_meta_path) {
                if let Ok(metadata_json) = serde_json::from_str::<serde_json::Value>(&metadata_str) {
                    metadata_json.get("content_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("application/octet-stream")
                        .to_string()
                } else {
                    "application/octet-stream".to_string()
                }
            } else {
                "application/octet-stream".to_string()
            };

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

            // Update quota and stats after successful multipart upload
            if let Err(e) = state.quota_manager.update_quota_add(&bucket, combined_data.len() as u64).await {
                warn!("Failed to update quota for bucket {} after multipart upload: {}", bucket, e);
            }
            if let Err(e) = state.quota_manager.increment_stat(&bucket, Operation::Multipart).await {
                warn!("Failed to update multipart stats for bucket {}: {}", bucket, e);
            }

            // Save object metadata
            let metadata_path = state.storage_path.join(&bucket).join(format!("{}.metadata", key));
            let metadata = ObjectMetadata {
                key: key.clone(),
                size: combined_data.len() as u64,
                etag: etag.clone(),
                last_modified: Utc::now(),
                content_type: stored_content_type, // Use the content type from initiation
                storage_class: "STANDARD".to_string(),
                metadata: HashMap::new(),
                version_id: None,
                encryption: None, // TODO: Add encryption support for multipart
                tags: None,
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

            // Note: In-memory metadata tracking removed - using filesystem-only approach

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
pub async fn handle_object_delete(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(params): Query<ObjectQueryParams>,
) -> impl IntoResponse {
    info!("DELETE object: {}/{} with params: {:?}", bucket, key, params);
    info!("version_id specifically: {:?}", params.version_id);

    if params.tagging.is_some() {
        // Delete object tags from metadata
        let metadata_path = state.storage_path.join(&bucket).join(format!("{}.metadata", key));

        if metadata_path.exists() {
            // Read existing metadata
            match fs::read_to_string(&metadata_path) {
                Ok(content) => {
                    match serde_json::from_str::<ObjectMetadata>(&content) {
                        Ok(mut metadata) => {
                            // Remove tags
                            metadata.tags = None;

                            // Write updated metadata
                            if let Err(e) = fs::write(&metadata_path, serde_json::to_string(&metadata).unwrap()) {
                                warn!("Failed to write metadata file: {}", e);
                                return Response::builder()
                                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                                    .body(Body::from("Failed to delete tags"))
                                    .unwrap();
                            }

                            info!("Tags deleted for object: {}/{}", bucket, key);
                        }
                        Err(e) => {
                            warn!("Failed to parse metadata file: {}", e);
                            return Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(Body::from("Failed to parse metadata"))
                                .unwrap();
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to read metadata file: {}", e);
                    return Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from("Failed to read metadata"))
                        .unwrap();
                }
            }
        }

        return Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Body::empty())
            .unwrap();
    }

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

    // Check if deleting a specific version
    if let Some(version_id) = &params.version_id {
        info!("Attempting to delete version {} of object {}/{}", version_id, bucket, key);
        if version_id != "null" {
            // Delete the specific version file
            let version_path = state.storage_path.join(&bucket).join(".versions").join(&key).join(version_id);
            let version_metadata_path = state.storage_path.join(&bucket).join(".versions").join(&key).join(format!("{}.metadata", version_id));

            info!("Version path: {:?}, exists: {}", version_path, version_path.exists());
            info!("Version metadata path: {:?}, exists: {}", version_metadata_path, version_metadata_path.exists());

            if version_path.exists() {
                // Delete version file
                if let Err(e) = fs::remove_file(&version_path) {
                    warn!("Failed to delete version file: {}", e);
                    return Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from("Failed to delete version"))
                        .unwrap();
                }

                // Delete version metadata file if it exists
                if version_metadata_path.exists() {
                    if let Err(e) = fs::remove_file(&version_metadata_path) {
                        warn!("Failed to delete version metadata: {}", e);
                    }
                }

                info!("Deleted version {} of object {}/{}", version_id, bucket, key);
                return StatusCode::NO_CONTENT.into_response();
            } else {
                // Version not found
                return Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("Version not found"))
                    .unwrap();
            }
        }
    }

    // Default: delete object
    delete_object(State(state), Path((bucket, key))).await.into_response()
}

pub async fn put_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    info!("Uploading object: {}/{}", bucket, key);

    // Check if this is a copy operation
    if let Some(copy_source) = headers.get("x-amz-copy-source") {
        let copy_source_str = copy_source.to_str().unwrap_or("");
        info!("Detected copy operation from source: {}", copy_source_str);

        // Parse the copy source (format: /bucket/key?versionId=xxx or bucket/key?versionId=xxx)
        let source_path = if copy_source_str.starts_with('/') {
            &copy_source_str[1..]
        } else {
            copy_source_str
        };

        // Check for versionId parameter
        let (base_path, version_id) = if let Some(pos) = source_path.find("?versionId=") {
            let (base, query) = source_path.split_at(pos);
            let vid = &query[11..]; // Skip "?versionId="
            (base, Some(vid.to_string()))
        } else {
            (source_path, None)
        };

        let parts: Vec<&str> = base_path.splitn(2, '/').collect();
        if parts.len() != 2 {
            warn!("Invalid copy source format: {}", copy_source_str);
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from("InvalidArgument: Invalid copy source"))
                .unwrap();
        }

        let source_bucket = parts[0];
        let source_key = parts[1];

        // URL decode the source key if needed
        let decoded_source_key = urlencoding::decode(source_key)
            .unwrap_or_else(|_| std::borrow::Cow::Borrowed(source_key))
            .into_owned();

        info!("Copying from bucket: {} key: {} version: {:?} to bucket: {} key: {}",
              source_bucket, decoded_source_key, version_id, bucket, key);

        // Read the source object (with version support)
        let source_path = if let Some(ref vid) = version_id {
            if vid != "null" {
                state.storage_path.join(source_bucket).join(".versions").join(&decoded_source_key).join(vid)
            } else {
                state.storage_path.join(source_bucket).join(&decoded_source_key)
            }
        } else {
            state.storage_path.join(source_bucket).join(&decoded_source_key)
        };

        let source_metadata_path = if let Some(ref vid) = version_id {
            if vid != "null" {
                state.storage_path.join(source_bucket).join(".versions").join(&decoded_source_key).join(format!("{}.metadata", vid))
            } else {
                state.storage_path.join(source_bucket).join(format!("{}.metadata", &decoded_source_key))
            }
        } else {
            state.storage_path.join(source_bucket).join(format!("{}.metadata", &decoded_source_key))
        };

        match fs::read(&source_path) {
            Ok(source_data) => {
                // Use the source data for the copy
                let data = source_data;
                let etag = format!("{:x}", md5::compute(&data));

                // Continue with normal put operation using the copied data
                let bucket_path = state.storage_path.join(&bucket);
                if let Err(e) = fs::create_dir_all(&bucket_path) {
                    warn!("Failed to create bucket directory: {}", e);
                }

                let object_path = bucket_path.join(&key);
                let dest_metadata_path = bucket_path.join(format!("{}.metadata", &key));

                // Create parent directory if needed
                if let Some(parent) = object_path.parent() {
                    if let Err(e) = fs::create_dir_all(parent) {
                        warn!("Failed to create object parent directory: {}", e);
                    }
                }

                // Write the copied data
                if let Err(e) = fs::write(&object_path, &data) {
                    warn!("Failed to write copied object: {}", e);
                    return Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from("Failed to copy object"))
                        .unwrap();
                }

                // Check for metadata directive
                let metadata_directive = headers
                    .get("x-amz-metadata-directive")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("COPY");

                // Extract custom metadata from headers
                let mut custom_metadata = HashMap::new();
                for (name, value) in &headers {
                    let key_str = name.as_str();
                    if key_str.starts_with("x-amz-meta-") {
                        if let Ok(value_str) = value.to_str() {
                            let meta_key = key_str.strip_prefix("x-amz-meta-").unwrap();
                            custom_metadata.insert(meta_key.to_string(), value_str.to_string());
                            debug!("Found custom metadata: {} = {}", meta_key, value_str);
                        }
                    }
                }

                // Copy metadata file if it exists, or create new metadata
                let content_type = if source_metadata_path.exists() {
                    // Read and copy the metadata, updating the key
                    match fs::read_to_string(&source_metadata_path) {
                        Ok(metadata_str) => {
                            if let Ok(mut metadata) = serde_json::from_str::<ObjectMetadata>(&metadata_str) {
                                // Update the metadata for the new location
                                metadata.key = key.clone();
                                metadata.last_modified = Utc::now();
                                metadata.etag = etag.clone();

                                // Handle metadata directive
                                if metadata_directive == "REPLACE" {
                                    // Replace all custom metadata with new ones
                                    metadata.metadata = custom_metadata.clone();
                                    info!("REPLACE directive: replacing metadata with {:?}", custom_metadata);
                                } else {
                                    // COPY directive: merge new metadata with existing
                                    for (k, v) in custom_metadata.iter() {
                                        metadata.metadata.insert(k.clone(), v.clone());
                                    }
                                }

                                // Update content-type if provided
                                if let Some(content_type_header) = headers.get(header::CONTENT_TYPE) {
                                    if let Ok(ct) = content_type_header.to_str() {
                                        metadata.content_type = ct.to_string();
                                    }
                                }

                                let ct = metadata.content_type.clone();

                                // Save the updated metadata
                                if let Ok(metadata_json) = serde_json::to_string(&metadata) {
                                    if let Err(e) = fs::write(&dest_metadata_path, metadata_json) {
                                        warn!("Failed to write copied metadata: {}", e);
                                    } else {
                                        debug!("Metadata copied to: {:?}", dest_metadata_path);
                                    }
                                }
                                ct
                            } else {
                                "application/octet-stream".to_string()
                            }
                        }
                        Err(e) => {
                            warn!("Failed to read source metadata: {}", e);
                            "application/octet-stream".to_string()
                        }
                    }
                } else {
                    // No metadata file exists, create basic metadata
                    let content_type_header = headers.get(header::CONTENT_TYPE)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("application/octet-stream")
                        .to_string();

                    let metadata = ObjectMetadata {
                        key: key.clone(),
                        size: data.len() as u64,
                        etag: etag.clone(),
                        last_modified: Utc::now(),
                        content_type: content_type_header.clone(),
                        storage_class: "STANDARD".to_string(),
                        metadata: custom_metadata, // Use the extracted custom metadata
                        version_id: None,
                        encryption: None,
                        tags: None,
                    };

                    if let Ok(metadata_json) = serde_json::to_string(&metadata) {
                        if let Err(e) = fs::write(&dest_metadata_path, metadata_json) {
                            warn!("Failed to write metadata: {}", e);
                        }
                    }
                    content_type_header
                };

                info!("Successfully copied object from {}/{} to {}/{} with content-type: {}",
                      source_bucket, decoded_source_key, bucket, key, content_type);

                // Update quota and stats after successful copy
                if let Err(e) = state.quota_manager.update_quota_add(&bucket, data.len() as u64).await {
                    warn!("Failed to update quota for bucket {} after copy: {}", bucket, e);
                }
                if let Err(e) = state.quota_manager.increment_stat(&bucket, Operation::Put).await {
                    warn!("Failed to update PUT stats for bucket {} after copy: {}", bucket, e);
                }

                // Return success response with ETag
                return Response::builder()
                    .status(StatusCode::OK)
                    .header(header::ETAG, format!("\"{}\"", etag))
                    .header("x-amz-copy-source-version-id", "null")
                    .body(Body::from(format!(
                        r#"<?xml version="1.0" encoding="UTF-8"?>
<CopyObjectResult>
    <LastModified>{}</LastModified>
    <ETag>"{}"</ETag>
</CopyObjectResult>"#,
                        Utc::now().to_rfc3339(),
                        etag
                    )))
                    .unwrap();
            }
            Err(e) => {
                warn!("Failed to read source object {}/{}: {}", source_bucket, decoded_source_key, e);
                return Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("NoSuchKey: The specified key does not exist"))
                    .unwrap();
            }
        }
    }

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

    // Handle folder creation (keys ending with / or empty)
    if key.ends_with('/') || key.is_empty() {
        // This is a folder creation request
        // Special case: if the key is empty or just "/" it refers to the bucket itself
        // which already exists after bucket creation, so just return success
        if key == "/" || key.is_empty() {
            info!("Bucket root folder already exists: {}", bucket);
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::ETAG, format!("\"{}\"", etag))
                .body(Body::empty())
                .unwrap();
        }

        if let Err(e) = fs::create_dir_all(&object_path) {
            warn!("Failed to create folder: {}", e);
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("Failed to create folder"))
                .unwrap();
        }

        info!("Created folder: {}/{}", bucket, key);

        // Return success for folder creation
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::ETAG, format!("\"{}\"", etag))
            .body(Body::empty())
            .unwrap();
    }

    // Regular file handling
    if let Some(parent) = object_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            warn!("Failed to create object parent directory: {}", e);
        }
    }

    // Check if versioning is enabled for this bucket
    let version_id = {
        let versioning_status = read_bucket_versioning(&state.storage_path, &bucket);
        let versioning_enabled = versioning_status
            .as_ref()
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

            // Save metadata for this version
            let version_metadata_path = versions_dir.join(format!("{}.metadata", &vid));

            // Get content type from headers
            let version_content_type = headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();

            // Extract custom metadata for this version
            let mut version_custom_metadata = HashMap::new();
            for (name, value) in &headers {
                let key_str = name.as_str();
                if key_str.starts_with("x-amz-meta-") {
                    if let Ok(value_str) = value.to_str() {
                        let meta_key = key_str.strip_prefix("x-amz-meta-").unwrap();
                        version_custom_metadata.insert(meta_key.to_string(), value_str.to_string());
                    }
                }
            }

            // Note: For now, we'll save version metadata without encryption info
            // The version data is saved unencrypted in the current implementation
            // TODO: Consider encrypting version data if bucket has encryption enabled
            let version_metadata = ObjectMetadata {
                key: key.clone(),
                size: data.len() as u64,
                etag: etag.clone(),
                last_modified: Utc::now(),
                content_type: version_content_type,
                storage_class: "STANDARD".to_string(),
                metadata: version_custom_metadata,
                version_id: Some(vid.clone()),
                encryption: None, // Versions are not encrypted in current implementation
                tags: None, // TODO: Copy tags from current version if they exist
            };

            if let Ok(metadata_json) = serde_json::to_string(&version_metadata) {
                if let Err(e) = fs::write(&version_metadata_path, metadata_json) {
                    warn!("Failed to write version metadata: {}", e);
                } else {
                    debug!("Version metadata saved to: {:?}", version_metadata_path);
                }
            }

            // Note: Version tracking is now filesystem-only (no in-memory tracking)

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
        if let Some(encryption) = read_bucket_encryption(&state.storage_path, &bucket) {
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
        tags: None,
    };

    if let Ok(metadata_json) = serde_json::to_string(&metadata) {
        if let Err(e) = fs::write(&metadata_path, metadata_json) {
            warn!("Failed to write metadata file: {}", e);
        } else {
            debug!("Metadata saved to: {:?}", metadata_path);
        }
    }

    info!("Object stored at: {:?}", object_path);

    // Update quota and stats after successful write
    if let Err(e) = state.quota_manager.update_quota_add(&bucket, final_data.len() as u64).await {
        warn!("Failed to update quota for bucket {}: {}", bucket, e);
    }
    if let Err(e) = state.quota_manager.increment_stat(&bucket, Operation::Put).await {
        warn!("Failed to update PUT stats for bucket {}: {}", bucket, e);
    }

    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(header::ETAG, format!("\"{}\"", etag));

    // Add version ID header if versioning is enabled
    if let Some(ref vid) = version_id {
        response = response.header("x-amz-version-id", vid);
    }

    response.body(Body::empty()).unwrap()
}

pub async fn get_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    version_id: Option<String>,
) -> impl IntoResponse {
    debug!("Getting object: {}/{} version: {:?}", bucket, key, version_id);

    // Determine which file to read based on version_id
    let object_path = if let Some(ref vid) = version_id {
        if vid != "null" {
            // Read from version directory
            state.storage_path.join(&bucket).join(".versions").join(&key).join(vid)
        } else {
            // "null" means current version
            state.storage_path.join(&bucket).join(&key)
        }
    } else {
        // No version specified, read current
        state.storage_path.join(&bucket).join(&key)
    };

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
    let metadata_path = if let Some(ref vid) = version_id {
        if vid != "null" {
            // Read metadata from version directory
            state.storage_path.join(&bucket).join(".versions").join(&key).join(format!("{}.metadata", vid))
        } else {
            // Current version metadata
            state.storage_path.join(&bucket).join(format!("{}.metadata", key))
        }
    } else {
        state.storage_path.join(&bucket).join(format!("{}.metadata", key))
    };
    let (data_to_return, etag, last_modified, content_type, encryption_header, custom_metadata) = if let Ok(metadata_json) = fs::read_to_string(&metadata_path) {
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
            (final_data, metadata.etag, metadata.last_modified, metadata.content_type, enc_header, metadata.metadata)
        } else {
            // Metadata file exists but couldn't parse, fall back to defaults
            let etag = format!("{:x}", md5::compute(&data));
            (data.clone(), etag, Utc::now(), "application/octet-stream".to_string(), None, HashMap::new())
        }
    } else {
        // No metadata file, calculate etag from file data
        let etag = format!("{:x}", md5::compute(&data));
        (data.clone(), etag, Utc::now(), "application/octet-stream".to_string(), None, HashMap::new())
    };

    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, data_to_return.len().to_string())
        .header(header::ETAG, format!("\"{}\"", etag))
        .header(header::LAST_MODIFIED, format_http_date(&last_modified));

    // Add custom metadata headers
    for (key, value) in custom_metadata {
        let header_name = format!("x-amz-meta-{}", key);
        response = response.header(header_name, value);
    }

    // Add encryption header if object was encrypted
    if let Some(enc_algorithm) = encryption_header {
        response = response.header("x-amz-server-side-encryption", enc_algorithm);
    }

    response.body(Body::from(data_to_return)).unwrap()
}

pub async fn delete_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> impl IntoResponse {
    info!("Deleting object: {}/{}", bucket, key);

    // Increment stats for DELETE operation
    if let Err(e) = state.quota_manager.increment_stat(&bucket, Operation::Delete).await {
        warn!("Failed to update DELETE stats for bucket {}: {}", bucket, e);
    }

    // Check if the path is a directory
    let object_path = state.storage_path.join(&bucket).join(&key);

    // Get object size before deletion for quota update (only if it's a file)
    let object_size = if object_path.is_file() {
        fs::metadata(&object_path).ok().map(|m| m.len())
    } else {
        None
    };

    // If path ends with '/' or is a directory, handle it as a prefix deletion
    if key.ends_with('/') || object_path.is_dir() {
        // In S3, deleting a "directory" (prefix) succeeds if it's empty
        // For filesystem-based storage, we try to remove the directory
        if object_path.is_dir() {
            // First try to remove as empty directory
            match fs::remove_dir(&object_path) {
                Ok(_) => {
                    info!("Deleted empty directory: {}/{}", bucket, key);
                    return StatusCode::NO_CONTENT;
                }
                Err(_) => {
                    // If directory is not empty, recursively delete all contents
                    match fs::remove_dir_all(&object_path) {
                        Ok(_) => {
                            info!("Deleted directory and all contents: {}/{}", bucket, key);
                            return StatusCode::NO_CONTENT;
                        }
                        Err(e) => {
                            warn!("Failed to delete directory {}/{}: {}", bucket, key, e);
                            // In S3, attempting to delete a non-existent prefix returns 204
                            return StatusCode::NO_CONTENT;
                        }
                    }
                }
            }
        } else {
            // Path doesn't exist, but in S3 this is still successful
            return StatusCode::NO_CONTENT;
        }
    }

    // Delete from disk - check if it's a file or directory
    let disk_deleted = if object_path.is_dir() {
        // If it's a directory, try to remove it (only if empty)
        // In S3, directories are just prefixes, so we can safely ignore directory deletions
        fs::remove_dir(&object_path).is_ok() || true  // Always treat directory deletion as successful
    } else {
        // If it's a file, remove it normally
        fs::remove_file(&object_path).is_ok()
    };

    // Also delete metadata file
    // Metadata is stored as filename.ext.metadata (not filename.metadata)
    let metadata_path = state.storage_path.join(&bucket).join(format!("{}.metadata", key));
    let metadata_deleted = fs::remove_file(&metadata_path).is_ok();

    if metadata_deleted {
        debug!("Deleted metadata file for {}/{}", bucket, key);
    }

    // Update quota if we successfully deleted a file
    if disk_deleted && !object_path.is_dir() {
        // Always update quota for successful file deletion
        // Use the size if we have it, otherwise use 0 (object count will still be decremented)
        let size_to_remove = object_size.unwrap_or(0);
        if let Err(e) = state.quota_manager.update_quota_remove(&bucket, size_to_remove).await {
            warn!("Failed to update quota for bucket {} after deletion: {}", bucket, e);
        }
    }

    if disk_deleted || metadata_deleted {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

pub async fn head_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> impl IntoResponse {
    // Increment stats for HEAD operation
    if let Err(e) = state.quota_manager.increment_stat(&bucket, Operation::Head).await {
        warn!("Failed to update HEAD stats for bucket {}: {}", bucket, e);
    }

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
    let (size, etag, last_modified, content_type, custom_metadata) = if let Ok(metadata_json) = fs::read_to_string(&metadata_path) {
        if let Ok(metadata) = serde_json::from_str::<ObjectMetadata>(&metadata_json) {
            (metadata.size, metadata.etag, metadata.last_modified, metadata.content_type, metadata.metadata)
        } else {
            // Metadata file exists but couldn't parse, fall back to file stats
            let file_metadata = fs::metadata(&object_path).unwrap();
            let size = file_metadata.len();
            let data = fs::read(&object_path).unwrap_or_default();
            let etag = format!("{:x}", md5::compute(&data));
            (size, etag, Utc::now(), "application/octet-stream".to_string(), HashMap::new())
        }
    } else {
        // No metadata file, use file stats
        let file_metadata = fs::metadata(&object_path).unwrap();
        let size = file_metadata.len();
        let data = fs::read(&object_path).unwrap_or_default();
        let etag = format!("{:x}", md5::compute(&data));
        (size, etag, Utc::now(), "application/octet-stream".to_string(), HashMap::new())
    };

    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, size.to_string())
        .header(header::ETAG, format!("\"{}\"", etag))
        .header(header::LAST_MODIFIED, format_http_date(&last_modified));

    // Add custom metadata headers
    for (key, value) in custom_metadata {
        let header_name = format!("x-amz-meta-{}", key);
        response = response.header(header_name, value);
    }

    response.body(Body::empty()).unwrap()
}

// Helper functions for chunked data and encryption
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

    result
}

fn find_sequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn generate_encryption_key() -> Vec<u8> {
    let mut key = vec![0u8; 32]; // 256-bit key
    OsRng.fill_bytes(&mut key);
    key
}

fn encrypt_data(data: &[u8], key: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let key = Key::<Aes256Gcm>::from_slice(key);
    let cipher = Aes256Gcm::new(key);

    let mut nonce_bytes = vec![0u8; 12]; // 96-bit nonce
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    match cipher.encrypt(nonce, data) {
        Ok(ciphertext) => Ok((ciphertext, nonce_bytes)),
        Err(e) => Err(format!("Encryption failed: {}", e)),
    }
}

fn decrypt_data(ciphertext: &[u8], key: &[u8], nonce: &[u8]) -> Result<Vec<u8>, String> {
    let key = Key::<Aes256Gcm>::from_slice(key);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(nonce);

    match cipher.decrypt(nonce, ciphertext) {
        Ok(plaintext) => Ok(plaintext),
        Err(e) => Err(format!("Decryption failed: {}", e)),
    }
}