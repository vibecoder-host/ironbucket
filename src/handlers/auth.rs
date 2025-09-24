use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Method, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use tracing::{debug, info};

use crate::{AppState, check_policy_permission, filesystem::read_bucket_policy};

pub async fn auth_middleware(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request<Body>,
    next: Next,
) -> Response {
    // Extract client IP from headers, defaulting to localhost if not found
    let client_ip = headers.get("x-real-ip")
        .or_else(|| headers.get("x-forwarded-for"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
        .or_else(|| Some("127.0.0.1".to_string()));  // Default to localhost for direct connections

    // Log all requests for debugging
    debug!("Request: {:?} {} {:?} from IP: {:?}", request.method(), request.uri(), headers, client_ip);

    // Extract bucket name from the path
    let path = request.uri().path();
    let bucket_name = if path != "/" {
        path.trim_start_matches('/').split('/').next()
    } else {
        None
    };

    // Determine S3 action from request method and path
    let action = match request.method() {
        &Method::GET => "s3:GetObject",
        &Method::PUT => "s3:PutObject",
        &Method::DELETE => "s3:DeleteObject",
        &Method::HEAD => "s3:GetObject",
        _ => "s3:*",
    };

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

                        // Check bucket policy with IP conditions
                        if let Some(bucket) = bucket_name {
                            // Read policy from filesystem
                            let policy_json = read_bucket_policy(&state.storage_path, bucket);

                            if let Some(ref policy_str) = policy_json {
                                let resource = format!("arn:aws:s3:::{}/{}*", bucket,
                                    path.trim_start_matches('/').trim_start_matches(bucket).trim_start_matches('/'));

                                let allowed = check_policy_permission(
                                    policy_str,
                                    action,
                                    &resource,
                                    "*", // Principal for presigned URLs
                                    client_ip.as_deref()
                                );

                                if !allowed {
                                    info!("Access denied by bucket policy for presigned URL: bucket={}, action={}, client_ip={:?}",
                                          bucket, action, client_ip);
                                    return Response::builder()
                                        .status(StatusCode::FORBIDDEN)
                                        .body(Body::from("Access Denied by bucket policy"))
                                        .unwrap();
                                }
                            }
                        }

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

                                // Check bucket policy with IP conditions
                                if let Some(bucket) = bucket_name {
                                    // Read policy from filesystem
                                    let policy_json = read_bucket_policy(&state.storage_path, bucket);

                                    if let Some(ref policy_str) = policy_json {
                                        let resource = format!("arn:aws:s3:::{}/{}*", bucket,
                                            path.trim_start_matches('/').trim_start_matches(bucket).trim_start_matches('/'));

                                        let allowed = check_policy_permission(
                                            policy_str,
                                            action,
                                            &resource,
                                            access_key, // Use actual access key as principal
                                            client_ip.as_deref()
                                        );

                                        if !allowed {
                                            info!("Access denied by bucket policy: bucket={}, action={}, client_ip={:?}",
                                                  bucket, action, client_ip);
                                            return Response::builder()
                                                .status(StatusCode::FORBIDDEN)
                                                .body(Body::from("Access Denied by bucket policy"))
                                                .unwrap();
                                        }
                                    }
                                }

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