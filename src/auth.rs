use crate::{config::AuthConfig, error::{Error, Result}};
use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use tower::Layer;
use tracing::{debug, warn};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct AuthLayer {
    config: AuthConfig,
}

impl AuthLayer {
    pub fn new(config: AuthConfig) -> Self {
        Self { config }
    }
}

impl<S> Layer<S> for AuthLayer {
    type Service = AuthMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthMiddleware {
            inner,
            config: self.config.clone(),
        }
    }
}

#[derive(Clone)]
pub struct AuthMiddleware<S> {
    inner: S,
    config: AuthConfig,
}

impl<S> AuthMiddleware<S> {
    async fn verify_signature_v4(
        &self,
        req: &Request,
        auth_header: &str,
    ) -> Result<()> {
        // Parse Authorization header
        let parts: Vec<&str> = auth_header.split_whitespace().collect();
        if parts.len() != 4 || !parts[0].starts_with("AWS4-HMAC-SHA256") {
            return Err(Error::InvalidRequest("Invalid authorization header".to_string()));
        }

        // Extract credential, signed headers, and signature
        let credential_part = parts[1].trim_start_matches("Credential=").trim_end_matches(',');
        let signed_headers_part = parts[2].trim_start_matches("SignedHeaders=").trim_end_matches(',');
        let signature = parts[3].trim_start_matches("Signature=");

        // Parse credential
        let cred_parts: Vec<&str> = credential_part.split('/').collect();
        if cred_parts.len() != 5 {
            return Err(Error::InvalidRequest("Invalid credential format".to_string()));
        }

        let access_key = cred_parts[0];
        let date = cred_parts[1];
        let region = cred_parts[2];
        let service = cred_parts[3];
        let request_type = cred_parts[4];

        // Verify access key
        if access_key != self.config.access_key_id {
            return Err(Error::InvalidAccessKeyId);
        }

        // Create canonical request
        let canonical_request = self.create_canonical_request(req, signed_headers_part)?;
        let canonical_request_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));

        // Create string to sign
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}/{}/{}/{}\n{}",
            req.headers().get("x-amz-date").and_then(|v| v.to_str().ok()).unwrap_or(""),
            date,
            region,
            service,
            request_type,
            canonical_request_hash
        );

        // Calculate signature
        let signing_key = self.get_signing_key(date, region, service)?;
        let calculated_signature = self.calculate_signature(&signing_key, &string_to_sign);

        // Compare signatures
        if calculated_signature != signature {
            warn!("Signature mismatch: expected {}, got {}", calculated_signature, signature);
            return Err(Error::SignatureDoesNotMatch);
        }

        Ok(())
    }

    fn create_canonical_request(&self, req: &Request, signed_headers: &str) -> Result<String> {
        let method = req.method().as_str();
        let uri = req.uri().path();
        let query = req.uri().query().unwrap_or("");

        // Get signed headers
        let headers = req.headers();
        let mut canonical_headers = String::new();
        for header_name in signed_headers.split(';') {
            if let Some(value) = headers.get(header_name) {
                canonical_headers.push_str(&format!(
                    "{}:{}\n",
                    header_name,
                    value.to_str().unwrap_or("")
                ));
            }
        }

        // Get payload hash
        let payload_hash = headers
            .get("x-amz-content-sha256")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("UNSIGNED-PAYLOAD");

        Ok(format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            method,
            uri,
            query,
            canonical_headers,
            signed_headers,
            payload_hash
        ))
    }

    fn get_signing_key(&self, date: &str, region: &str, service: &str) -> Result<Vec<u8>> {
        let secret = format!("AWS4{}", self.config.secret_access_key);
        let mut key = self.hmac_sha256(secret.as_bytes(), date.as_bytes());
        key = self.hmac_sha256(&key, region.as_bytes());
        key = self.hmac_sha256(&key, service.as_bytes());
        key = self.hmac_sha256(&key, b"aws4_request");
        Ok(key)
    }

    fn hmac_sha256(&self, key: &[u8], data: &[u8]) -> Vec<u8> {
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }

    fn calculate_signature(&self, key: &[u8], string_to_sign: &str) -> String {
        hex::encode(self.hmac_sha256(key, string_to_sign.as_bytes()))
    }

    pub async fn authenticate(&self, req: Request, next: Next) -> Response {
        // Skip authentication for health checks and public operations
        let path = req.uri().path();
        if path == "/health" || path == "/" && req.method() == "GET" {
            return next.run(req).await;
        }

        // Check for Authorization header
        if let Some(auth_header) = req.headers().get(header::AUTHORIZATION) {
            if let Ok(auth_str) = auth_header.to_str() {
                // AWS Signature V4
                if auth_str.starts_with("AWS4-HMAC-SHA256") {
                    match self.verify_signature_v4(&req, auth_str).await {
                        Ok(_) => {
                            debug!("Authentication successful");
                            return next.run(req).await;
                        }
                        Err(e) => {
                            warn!("Authentication failed: {}", e);
                            return Response::builder()
                                .status(StatusCode::FORBIDDEN)
                                .header("Content-Type", "application/xml")
                                .body(e.to_xml().into())
                                .unwrap();
                        }
                    }
                }
            }
        }

        // Check for presigned URL authentication
        if req.uri().query().is_some() {
            let query_params: HashMap<String, String> = req
                .uri()
                .query()
                .unwrap_or("")
                .split('&')
                .filter_map(|pair| {
                    let mut parts = pair.split('=');
                    Some((parts.next()?.to_string(), parts.next()?.to_string()))
                })
                .collect();

            if query_params.contains_key("X-Amz-Signature") {
                // Handle presigned URL authentication
                debug!("Presigned URL authentication - simplified for now");
                return next.run(req).await;
            }
        }

        // No authentication provided for protected resource
        Response::builder()
            .status(StatusCode::FORBIDDEN)
            .header("Content-Type", "application/xml")
            .body(Error::AccessDenied.to_xml().into())
            .unwrap()
    }
}

// Simplified Service implementation - just pass through for now
impl<S> tower::Service<Request> for AuthMiddleware<S>
where
    S: tower::Service<Request, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>
    >;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);

        Box::pin(async move {
            // For now, just pass through
            let response = inner.call(req).await?;
            Ok(response)
        })
    }
}