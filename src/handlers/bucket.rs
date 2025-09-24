use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use chrono::{Utc, TimeZone};
use serde::Deserialize;
use std::{collections::HashSet, fs};
use tracing::{debug, info, warn, error};

use crate::{
    AppState, BucketEncryption, CorsConfiguration, CorsRule, LifecycleConfiguration,
    LifecycleRule, LifecycleFilter, LifecycleTag, LifecycleExpiration, LifecycleTransition,
    ObjectData,
    // Import filesystem functions
    bucket_exists, read_bucket_versioning, read_bucket_policy, read_bucket_encryption,
    read_bucket_cors, read_bucket_lifecycle, write_bucket_versioning, write_bucket_policy,
    write_bucket_encryption, write_bucket_cors, write_bucket_lifecycle,
    delete_bucket_policy, delete_bucket_encryption, delete_bucket_cors, delete_bucket_lifecycle
};

// Query parameters for bucket operations
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
    #[serde(rename = "max-keys")]
    pub max_keys: Option<usize>,
    pub prefix: Option<String>,
    #[serde(rename = "continuation-token")]
    pub continuation_token: Option<String>,
    pub delimiter: Option<String>,
    #[serde(rename = "list-type")]
    pub list_type: Option<String>,
}

// Handle bucket GET with query parameters
pub async fn handle_bucket_get(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Query(params): Query<BucketQueryParams>,
) -> impl IntoResponse {
    debug!("GET bucket: {} with params: {:?}", bucket, params);

    // Check if bucket exists on filesystem
    if !bucket_exists(&state.storage_path, &bucket) {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap();
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
        // Return versioning status from filesystem
        let status = read_bucket_versioning(&state.storage_path, &bucket);

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
        // Return bucket policy from filesystem
        if let Some(policy) = read_bucket_policy(&state.storage_path, &bucket) {
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(policy))
                .unwrap();
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
        // Return bucket encryption configuration from filesystem
        if let Some(encryption) = read_bucket_encryption(&state.storage_path, &bucket) {
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
        // Return bucket CORS configuration from filesystem
        if let Some(cors) = read_bucket_cors(&state.storage_path, &bucket) {
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
        // Return bucket lifecycle configuration from filesystem
        if let Some(lifecycle) = read_bucket_lifecycle(&state.storage_path, &bucket) {
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
        // Check if bucket exists
        if !bucket_exists(&state.storage_path, &bucket) {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap();
        }

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

        // TODO: Implement filesystem-based object version listing
        // This functionality is disabled because it references the old in-memory bucket_data
        // which has been removed in favor of filesystem operations. When object handlers
        // are refactored to use filesystem operations, this should be reimplemented to:
        // 1. Scan filesystem for objects and their metadata files
        // 2. Read version information from .metadata files
        // 3. List both current objects and versioned objects from filesystem
        // 4. Support proper pagination and filtering
        if false { // Disabled - references removed bucket_data.objects and bucket_data.versions
            // This block contains references to bucket_data.objects and bucket_data.versions
            // which no longer exist since we moved to filesystem-only operations.
            // The implementation needs to be rewritten to use filesystem scanning.
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
pub async fn handle_bucket_put(
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

        // Check if bucket exists first
        if !bucket_exists(&state.storage_path, &bucket) {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("NoSuchBucket"))
                .unwrap();
        }

        // Update bucket versioning status directly on filesystem
        if let Some(ref status) = status {
            if let Err(e) = write_bucket_versioning(&state.storage_path, &bucket, status) {
                warn!("Failed to persist versioning status: {}", e);
                return Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("InternalError"))
                    .unwrap();
            }
        }
        info!("Set versioning status for bucket {} to {:?}", bucket, status);

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

        // Check if bucket exists first
        if !bucket_exists(&state.storage_path, &bucket) {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("NoSuchBucket"))
                .unwrap();
        }

        // Update bucket policy directly on filesystem
        if let Err(e) = write_bucket_policy(&state.storage_path, &bucket, &policy_str) {
            warn!("Failed to persist bucket policy: {}", e);
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("InternalError"))
                .unwrap();
        }
        info!("Set policy for bucket {}", bucket);

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

        // Check if bucket exists first
        if !bucket_exists(&state.storage_path, &bucket) {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("NoSuchBucket"))
                .unwrap();
        }

        // Update bucket encryption directly on filesystem
        let encryption = BucketEncryption {
            algorithm: algorithm.clone(),
            kms_key_id: kms_key_id.clone(),
        };
        if let Err(e) = write_bucket_encryption(&state.storage_path, &bucket, &encryption) {
            warn!("Failed to persist encryption configuration: {}", e);
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("InternalError"))
                .unwrap();
        }
        info!("Set encryption for bucket {}: {}", bucket, algorithm);

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

        // Check if bucket exists first
        if !bucket_exists(&state.storage_path, &bucket) {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("NoSuchBucket"))
                .unwrap();
        }

        // Store CORS configuration using filesystem function
        let cors_config = CorsConfiguration {
            cors_rules,
        };

        if let Err(e) = write_bucket_cors(&state.storage_path, &bucket, &cors_config) {
            warn!("Failed to persist CORS configuration: {}", e);
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("InternalError"))
                .unwrap();
        }

        info!("Set CORS configuration for bucket {}", bucket);

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

        // Check if bucket exists first
        if !bucket_exists(&state.storage_path, &bucket) {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("NoSuchBucket"))
                .unwrap();
        }

        // Store lifecycle configuration using filesystem function
        if let Err(e) = write_bucket_lifecycle(&state.storage_path, &bucket, &lifecycle_config) {
            warn!("Failed to persist lifecycle configuration: {}", e);
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("InternalError"))
                .unwrap();
        }

        info!("Set lifecycle configuration for bucket {}", bucket);

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
pub async fn handle_bucket_post(
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

        // Parse objects to delete from XML body
        let object_parts: Vec<&str> = body_str.split("<Object>").collect();
        for (i, object_part) in object_parts.iter().enumerate() {
            if i == 0 { continue; } // Skip the part before first Object

            let mut key = String::new();
            let mut version_id = None;

            // Extract Key
            if let Some(key_start) = object_part.find("<Key>") {
                if let Some(key_end) = object_part.find("</Key>") {
                    key = object_part[key_start + 5..key_end].to_string();
                }
            }

            // Extract VersionId if present
            if let Some(version_start) = object_part.find("<VersionId>") {
                if let Some(version_end) = object_part.find("</VersionId>") {
                    version_id = Some(object_part[version_start + 11..version_end].to_string());
                }
            }

            if !key.is_empty() {
                objects_to_delete.push(DeleteObject { key, version_id });
            }
        }

        debug!("Parsed {} objects to delete", objects_to_delete.len());

        // Process each delete request
        for delete_obj in objects_to_delete {
            let object_path = state.storage_path.join(&bucket).join(&delete_obj.key);
            let metadata_path = state.storage_path.join(&bucket).join(format!("{}.metadata", delete_obj.key));

            if object_path.exists() {
                // Check if it's a directory or a file
                let deletion_result = if object_path.is_dir() {
                    // If it's a directory, try to remove it (only if empty)
                    fs::remove_dir(&object_path)
                } else {
                    // If it's a file, remove it normally
                    fs::remove_file(&object_path)
                };

                match deletion_result {
                    Ok(_) => {
                        // Also remove metadata file if it exists
                        if metadata_path.exists() {
                            let _ = fs::remove_file(&metadata_path);
                        }

                        // TODO: Remove from any object indexes when implemented
                        // For now, filesystem deletion is sufficient

                        result.deleted.push(DeletedObject {
                            key: delete_obj.key.clone(),
                            version_id: delete_obj.version_id.clone(),
                            delete_marker: false,
                            delete_marker_version_id: None,
                        });
                        debug!("Successfully deleted object: {}", delete_obj.key);
                    }
                    Err(e) => {
                        // Only log as error if it's not a non-empty directory
                        if object_path.is_dir() {
                            // For directories, we'll treat them as successfully deleted
                            // S3 doesn't really have directories, they're just prefixes
                            result.deleted.push(DeletedObject {
                                key: delete_obj.key.clone(),
                                version_id: delete_obj.version_id.clone(),
                                delete_marker: false,
                                delete_marker_version_id: None,
                            });
                            debug!("Skipped directory deletion for: {} (S3 treats directories as prefixes)", delete_obj.key);
                        } else {
                            result.errors.push(DeleteError {
                                key: delete_obj.key.clone(),
                                code: "InternalError".to_string(),
                                message: format!("Failed to delete object: {}", e),
                                version_id: delete_obj.version_id,
                            });
                            warn!("Failed to delete object {}: {}", delete_obj.key, e);
                        }
                    }
                }
            } else {
                // Object doesn't exist - this is not an error in S3, just skip
                result.deleted.push(DeletedObject {
                    key: delete_obj.key.clone(),
                    version_id: delete_obj.version_id,
                    delete_marker: false,
                    delete_marker_version_id: None,
                });
                debug!("Object {} doesn't exist, treating as successful delete", delete_obj.key);
            }
        }

        // Generate response XML
        let mut response_xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<DeleteResult>"#);

        for deleted in result.deleted {
            response_xml.push_str(&format!(r#"
    <Deleted>
        <Key>{}</Key>"#, deleted.key));

            if let Some(ref version_id) = deleted.version_id {
                response_xml.push_str(&format!("
        <VersionId>{}</VersionId>", version_id));
            }

            if deleted.delete_marker {
                response_xml.push_str("
        <DeleteMarker>true</DeleteMarker>");
                if let Some(ref dm_version_id) = deleted.delete_marker_version_id {
                    response_xml.push_str(&format!("
        <DeleteMarkerVersionId>{}</DeleteMarkerVersionId>", dm_version_id));
                }
            }

            response_xml.push_str("
    </Deleted>");
        }

        for error in result.errors {
            response_xml.push_str(&format!(r#"
    <Error>
        <Key>{}</Key>
        <Code>{}</Code>
        <Message>{}</Message>"#, error.key, error.code, error.message));

            if let Some(ref version_id) = error.version_id {
                response_xml.push_str(&format!("
        <VersionId>{}</VersionId>", version_id));
            }

            response_xml.push_str("
    </Error>");
        }

        response_xml.push_str("\n</DeleteResult>");

        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/xml")
            .body(Body::from(response_xml))
            .unwrap();
    }

    // Default response for unhandled POST operations
    Response::builder()
        .status(StatusCode::OK)
        .body(Body::empty())
        .unwrap()
}

pub async fn create_bucket(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> impl IntoResponse {
    info!("Creating bucket: {}", bucket);

    let bucket_path = state.storage_path.join(&bucket);

    // Check if bucket already exists on filesystem
    if bucket_path.exists() {
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

    // Create the bucket directory on filesystem
    match fs::create_dir_all(&bucket_path) {
        Ok(_) => {
            info!("Successfully created bucket directory: {:?}", bucket_path);

            // Create a .bucket_metadata file to store bucket creation time and other metadata
            let metadata_path = bucket_path.join(".bucket_metadata");
            let metadata = serde_json::json!({
                "created": Utc::now().to_rfc3339(),
                "versioning_status": null,
            });

            if let Err(e) = fs::write(metadata_path, metadata.to_string()) {
                warn!("Failed to write bucket metadata: {}", e);
            }

            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_LENGTH, "0")
                .header("x-amz-request-id", "ironbucket-request-id")
                .header("x-amz-id-2", "ironbucket-id-2")
                .body(Body::empty())
                .unwrap()
        }
        Err(e) => {
            error!("Failed to create bucket directory: {}", e);
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header(header::CONTENT_TYPE, "application/xml")
                .body(Body::from(format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<Error>
    <Code>InternalError</Code>
    <Message>Failed to create bucket</Message>
    <BucketName>{}</BucketName>
</Error>"#, bucket)))
                .unwrap()
        }
    }
}

pub async fn delete_bucket(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Query(params): Query<BucketQueryParams>,
) -> impl IntoResponse {
    info!("Deleting bucket: {} with params: {:?}", bucket, params);

    // Handle policy deletion
    if params.policy.is_some() {
        // Check if bucket exists
        if !bucket_exists(&state.storage_path, &bucket) {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("NoSuchBucket"))
                .unwrap();
        }

        // Check if policy exists
        if read_bucket_policy(&state.storage_path, &bucket).is_some() {
            // Delete policy using filesystem function
            if let Err(e) = delete_bucket_policy(&state.storage_path, &bucket) {
                warn!("Failed to delete policy: {}", e);
                return Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("InternalError"))
                    .unwrap();
            }

            info!("Deleted policy for bucket {}", bucket);
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
    }

    // Handle encryption deletion
    if params.encryption.is_some() {
        // Check if bucket exists
        if !bucket_exists(&state.storage_path, &bucket) {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("NoSuchBucket"))
                .unwrap();
        }

        // Check if encryption exists
        if read_bucket_encryption(&state.storage_path, &bucket).is_some() {
            // Delete encryption using filesystem function
            if let Err(e) = delete_bucket_encryption(&state.storage_path, &bucket) {
                warn!("Failed to delete encryption configuration: {}", e);
                return Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("InternalError"))
                    .unwrap();
            }

            info!("Deleted encryption configuration for bucket {}", bucket);
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
    }

    // Handle CORS deletion
    if params.cors.is_some() {
        // Check if bucket exists
        if !bucket_exists(&state.storage_path, &bucket) {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("NoSuchBucket"))
                .unwrap();
        }

        // Check if CORS exists
        if read_bucket_cors(&state.storage_path, &bucket).is_some() {
            // Delete CORS using filesystem function
            if let Err(e) = delete_bucket_cors(&state.storage_path, &bucket) {
                warn!("Failed to delete CORS configuration: {}", e);
                return Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("InternalError"))
                    .unwrap();
            }

            info!("Deleted CORS configuration for bucket {}", bucket);
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
    }

    // Handle lifecycle deletion
    if params.lifecycle.is_some() {
        // Check if bucket exists
        if !bucket_exists(&state.storage_path, &bucket) {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("NoSuchBucket"))
                .unwrap();
        }

        // Check if lifecycle exists
        if read_bucket_lifecycle(&state.storage_path, &bucket).is_some() {
            // Delete lifecycle using filesystem function
            if let Err(e) = delete_bucket_lifecycle(&state.storage_path, &bucket) {
                warn!("Failed to delete lifecycle configuration: {}", e);
                return Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("InternalError"))
                    .unwrap();
            }

            info!("Deleted lifecycle configuration for bucket {}", bucket);
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
    }

    // Default: delete the bucket itself
    let bucket_path = state.storage_path.join(&bucket);

    // Check if bucket exists on filesystem
    if !bucket_path.exists() {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "application/xml")
            .body(Body::from(format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<Error>
    <Code>NoSuchBucket</Code>
    <Message>The specified bucket does not exist</Message>
    <BucketName>{}</BucketName>
</Error>"#, bucket)))
            .unwrap();
    }

    // Check if bucket is empty (S3 doesn't allow deleting non-empty buckets)
    // Only check if the directory exists on filesystem
    if bucket_path.exists() {
        if let Ok(entries) = fs::read_dir(&bucket_path) {
            let mut has_objects = false;
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    // Ignore hidden files like .policy, .cors, .multipart, etc.
                    if !name.starts_with('.') && !name.ends_with(".metadata") {
                        has_objects = true;
                        break;
                    }
                }
            }

            if has_objects {
                return Response::builder()
                    .status(StatusCode::CONFLICT)
                    .header(header::CONTENT_TYPE, "application/xml")
                    .body(Body::from(format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<Error>
    <Code>BucketNotEmpty</Code>
    <Message>The bucket you tried to delete is not empty</Message>
    <BucketName>{}</BucketName>
</Error>"#, bucket)))
                    .unwrap();
            }
        }
    }

    // Delete the bucket directory from filesystem
    match fs::remove_dir_all(&bucket_path) {
        Ok(_) => {
            info!("Successfully deleted bucket: {}", bucket);
            Response::builder()
                .status(StatusCode::NO_CONTENT)
                .body(Body::empty())
                .unwrap()
        }
        Err(e) => {
            error!("Failed to delete bucket {} from filesystem: {}", bucket, e);
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header(header::CONTENT_TYPE, "application/xml")
                .body(Body::from(format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<Error>
    <Code>InternalError</Code>
    <Message>Failed to delete bucket</Message>
    <BucketName>{}</BucketName>
</Error>"#, bucket)))
                .unwrap()
        }
    }
}

pub async fn head_bucket(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> impl IntoResponse {
    if bucket_exists(&state.storage_path, &bucket) {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

pub async fn list_objects_impl(
    state: State<AppState>,
    bucket: String,
    prefix: Option<String>,
    delimiter: Option<String>,
    continuation_token: Option<String>,
    max_keys: Option<usize>,
) -> Response {
    info!("Listing objects in bucket: {} with prefix: {:?}, delimiter: {:?}, continuation_token: {:?}, max_keys: {:?}",
           bucket, prefix, delimiter, continuation_token, max_keys);

    // First check if bucket exists on filesystem
    let bucket_path = state.storage_path.join(&bucket);
    if !bucket_path.exists() || !bucket_path.is_dir() {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap();
    }

    let prefix_str = prefix.as_deref().unwrap_or("");
    let max_keys = max_keys.unwrap_or(1000);

    // Collect all matching objects into a sorted vector for consistent pagination
    let _all_objects: Vec<(String, ObjectData)> = Vec::new();

    // Always scan filesystem for objects to ensure consistency
    // Use a recursive approach to handle prefixes that represent directories
    info!("Scanning filesystem for objects at: {:?} with prefix: {:?}", bucket_path, prefix_str);

    fn scan_directory(base_path: &std::path::Path, _current_prefix: &str, target_prefix: &str, _delimiter: Option<&str>) -> Vec<(String, ObjectData)> {
        let mut results = Vec::new();

        // Determine which directory to scan based on the prefix
        let scan_path = if !target_prefix.is_empty() && target_prefix.ends_with('/') {
            // If prefix ends with /, scan that subdirectory
            base_path.join(&target_prefix[..target_prefix.len()-1])
        } else if !target_prefix.is_empty() && target_prefix.contains('/') {
            // If prefix contains / but doesn't end with it, scan the parent directory
            let parts: Vec<&str> = target_prefix.rsplitn(2, '/').collect();
            if parts.len() == 2 {
                base_path.join(parts[1])
            } else {
                base_path.to_path_buf()
            }
        } else {
            base_path.to_path_buf()
        };

        if let Ok(entries) = fs::read_dir(&scan_path) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if let Some(name) = entry.file_name().to_str() {
                        // Skip metadata files and hidden files
                        if !name.ends_with(".metadata") && !name.starts_with(".") {
                            // Build the full key path relative to bucket
                            let relative_path = if let Ok(rel) = entry.path().strip_prefix(base_path) {
                                rel.to_string_lossy().to_string()
                            } else {
                                continue;
                            };

                            // Convert Windows paths to forward slashes
                            let mut key = relative_path.replace('\\', "/");

                            // Add trailing slash for directories
                            if metadata.is_dir() {
                                key.push('/');
                            }

                            // Check if this key matches our target prefix
                            if key.starts_with(target_prefix) {
                                let size = if metadata.is_file() {
                                    metadata.len() as usize
                                } else {
                                    0 // Directories have size 0
                                };

                                let last_modified = metadata.modified()
                                    .ok()
                                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                    .map(|d| Utc.timestamp_opt(d.as_secs() as i64, d.subsec_nanos()).unwrap())
                                    .unwrap_or_else(Utc::now);

                                let etag = format!("{:x}", md5::compute(format!("{}-{}", size, last_modified.timestamp()).as_bytes()));

                                results.push((key, ObjectData {
                                    data: Vec::new(),
                                    size,
                                    last_modified,
                                    etag,
                                }));
                            }
                        }
                    }
                }
            }
        }

        results
    }

    let mut all_objects = scan_directory(&bucket_path, "", prefix_str, delimiter.as_deref());
    let object_count = all_objects.len();
    info!("Scan complete: {} total objects matching prefix '{}' in bucket {}",
          object_count, prefix_str, bucket);

    all_objects.sort_by_key(|(key, _)| key.clone());

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
        page_objects.last().map(|(key, _)| key.to_string())
    } else {
        None
    };

    info!("Pagination debug: all_objects.len()={}, start_index={}, end_index={}, is_truncated={}, next_token={:?}",
           all_objects.len(), start_index, end_index, is_truncated, next_continuation_token);

    // Build common prefixes when delimiter is set
    let mut common_prefixes = Vec::new();
    if let Some(delim) = &delimiter {
        let mut seen_prefixes = HashSet::new();
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
    let mut xml = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
    <Name>{}</Name>
    <Prefix>{}</Prefix>
    <MaxKeys>{}</MaxKeys>
    <IsTruncated>{}</IsTruncated>"#,
        bucket,
        prefix_str,
        max_keys,
        if is_truncated { "true" } else { "false" }
    );

    if let Some(ref token) = next_continuation_token {
        xml.push_str(&format!("\n    <NextContinuationToken>{}</NextContinuationToken>", token));
    }

    xml.push_str(&format!("\n    <KeyCount>{}</KeyCount>", page_objects.len()));

    for (key, obj) in page_objects {
        xml.push_str(&format!(r#"
    <Contents>
        <Key>{}</Key>
        <LastModified>{}</LastModified>
        <ETag>"{}"</ETag>
        <Size>{}</Size>
        <StorageClass>STANDARD</StorageClass>
    </Contents>"#,
            key,
            obj.last_modified.to_rfc3339(),
            obj.etag,
            obj.size
        ));
    }

    for prefix in common_prefixes {
        xml.push_str(&format!(r#"
    <CommonPrefixes>
        <Prefix>{}</Prefix>
    </CommonPrefixes>"#, prefix));
    }

    xml.push_str("\n</ListBucketResult>");

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/xml")
        .body(Body::from(xml))
        .unwrap()
}