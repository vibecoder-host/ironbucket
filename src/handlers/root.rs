use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use chrono::{DateTime, Utc, TimeZone};
use std::fs;
use tracing::debug;

use crate::AppState;

pub async fn handle_root_post(
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

pub async fn list_buckets(State(state): State<AppState>) -> impl IntoResponse {
    debug!("Listing buckets");

    // Scan the filesystem for existing buckets (single source of truth)
    let mut all_buckets = Vec::new();

    // Read buckets from filesystem
    if let Ok(entries) = fs::read_dir(&state.storage_path) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        // Try to read bucket metadata file for creation time
                        let bucket_path = state.storage_path.join(name);
                        let metadata_path = bucket_path.join(".bucket_metadata");

                        let created = if metadata_path.exists() {
                            // Try to read the metadata file
                            fs::read_to_string(&metadata_path)
                                .ok()
                                .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
                                .and_then(|json| json.get("created")?.as_str().map(String::from))
                                .and_then(|date_str| DateTime::parse_from_rfc3339(&date_str).ok())
                                .map(|dt| dt.with_timezone(&Utc))
                                .unwrap_or_else(|| {
                                    // Fallback to filesystem metadata
                                    metadata.created()
                                        .ok()
                                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                        .map(|d| Utc.timestamp_opt(d.as_secs() as i64, d.subsec_nanos()).unwrap())
                                        .unwrap_or_else(Utc::now)
                                })
                        } else {
                            // Use filesystem metadata
                            metadata.created()
                                .ok()
                                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                .map(|d| Utc.timestamp_opt(d.as_secs() as i64, d.subsec_nanos()).unwrap())
                                .unwrap_or_else(Utc::now)
                        };

                        all_buckets.push((name.to_string(), created));
                    }
                }
            }
        }
    }

    // Sort buckets by name for consistent output
    all_buckets.sort_by(|a, b| a.0.cmp(&b.0));

    let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<ListAllMyBucketsResult>
    <Owner>
        <ID>ironbucket</ID>
        <DisplayName>IronBucket</DisplayName>
    </Owner>
    <Buckets>"#);

    for (name, created) in all_buckets {
        xml.push_str(&format!(
            r#"
        <Bucket>
            <Name>{}</Name>
            <CreationDate>{}</CreationDate>
        </Bucket>"#,
            name,
            created.to_rfc3339()
        ));
    }

    xml.push_str("\n    </Buckets>\n</ListAllMyBucketsResult>");

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/xml")
        .body(Body::from(xml))
        .unwrap()
}