# Rust Usage Guide

This guide demonstrates how to use IronBucket with Rust applications using the AWS SDK for Rust.

## Table of Contents

- [Installation](#installation)
- [Configuration](#configuration)
- [Basic Operations](#basic-operations)
- [Advanced Operations](#advanced-operations)
- [Multipart Upload](#multipart-upload)
- [Error Handling](#error-handling)
- [Async Operations](#async-operations)
- [Best Practices](#best-practices)
- [Complete Examples](#complete-examples)

## Installation

Add the AWS SDK for Rust to your `Cargo.toml`:

```toml
[dependencies]
aws-config = "1.5"
aws-sdk-s3 = "1.45"
tokio = { version = "1", features = ["full"] }

# Additional useful dependencies
bytes = "1.5"
futures = "0.3"
tracing = "0.1"
tracing-subscriber = "0.3"
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# For multipart uploads
aws-smithy-types = "1.2"
aws-smithy-runtime-api = "1.7"
```

## Configuration

### Basic Client Setup

```rust
use aws_config::{BehaviorVersion, Region};
use aws_sdk_s3::{Client, Config};
use aws_sdk_s3::config::{Credentials, SharedCredentialsProvider};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get credentials from environment variables
    let access_key = std::env::var("IRONBUCKET_ACCESS_KEY")
        .expect("IRONBUCKET_ACCESS_KEY must be set");
    let secret_key = std::env::var("IRONBUCKET_SECRET_KEY")
        .expect("IRONBUCKET_SECRET_KEY must be set");
    let endpoint = std::env::var("IRONBUCKET_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:9000".to_string());
    let region = std::env::var("IRONBUCKET_REGION")
        .unwrap_or_else(|_| "us-east-1".to_string());

    // Create credentials
    let credentials = Credentials::new(
        access_key,             // access_key
        secret_key,             // secret_key
        None,                   // session_token
        None,                   // expiration
        "IronBucket"           // provider_name
    );

    // Create S3 config
    let s3_config = Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .credentials_provider(SharedCredentialsProvider::new(credentials))
        .region(Region::new(region))
        .endpoint_url(endpoint)
        .force_path_style(true)
        .build();

    // Create S3 client
    let client = Client::from_conf(s3_config);

    Ok(())
}
```

### Environment Variables Configuration

```rust
use std::env;

async fn create_client_from_env() -> Result<Client, Box<dyn std::error::Error>> {
    // Read from environment
    let endpoint = env::var("IRONBUCKET_ENDPOINT").unwrap_or_else(|_| "http://localhost:9000".to_string());
    let access_key = env::var("IRONBUCKET_ACCESS_KEY")
        .expect("IRONBUCKET_ACCESS_KEY must be set");
    let secret_key = env::var("IRONBUCKET_SECRET_KEY")
        .expect("IRONBUCKET_SECRET_KEY must be set");
    let region = env::var("IRONBUCKET_REGION").unwrap_or_else(|_| "us-east-1".to_string());

    let credentials = Credentials::new(
        access_key,
        secret_key,
        None,
        None,
        "IronBucket"
    );

    let config = Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .credentials_provider(SharedCredentialsProvider::new(credentials))
        .region(Region::new(region))
        .endpoint_url(endpoint)
        .force_path_style(true)
        .build();

    Ok(Client::from_conf(config))
}
```

### Configuration Struct

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct S3Config {
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: String,
    pub region: String,
    pub bucket: String,
}

impl S3Config {
    pub fn from_env() -> Result<Self, env::VarError> {
        Ok(S3Config {
            endpoint: env::var("IRONBUCKET_ENDPOINT").unwrap_or_else(|_| "http://localhost:9000".to_string()),
            access_key: env::var("IRONBUCKET_ACCESS_KEY")?,
            secret_key: env::var("IRONBUCKET_SECRET_KEY")?,
            region: env::var("IRONBUCKET_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
            bucket: env::var("IRONBUCKET_BUCKET")?,
        })
    }

    pub async fn create_client(&self) -> Result<Client, Box<dyn std::error::Error>> {
        let credentials = Credentials::new(
            &self.access_key,
            &self.secret_key,
            None,
            None,
            "IronBucket"
        );

        let config = Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .credentials_provider(SharedCredentialsProvider::new(credentials))
            .region(Region::new(&self.region))
            .endpoint_url(&self.endpoint)
            .force_path_style(true)
            .build();

        Ok(Client::from_conf(config))
    }
}
```

## Basic Operations

### List Buckets

```rust
use aws_sdk_s3::Client;

async fn list_buckets(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    let resp = client.list_buckets().send().await?;

    println!("Buckets:");
    if let Some(buckets) = resp.buckets() {
        for bucket in buckets {
            println!("  - {} (Created: {:?})",
                bucket.name().unwrap_or("Unknown"),
                bucket.creation_date()
            );
        }
    }

    Ok(())
}
```

### Create Bucket

```rust
use aws_sdk_s3::types::BucketLocationConstraint;

async fn create_bucket(
    client: &Client,
    bucket_name: &str,
    region: &str
) -> Result<(), Box<dyn std::error::Error>> {
    let constraint = BucketLocationConstraint::from(region);
    let cfg = aws_sdk_s3::types::CreateBucketConfiguration::builder()
        .location_constraint(constraint)
        .build();

    client
        .create_bucket()
        .bucket(bucket_name)
        .create_bucket_configuration(cfg)
        .send()
        .await?;

    println!("Bucket '{}' created successfully", bucket_name);
    Ok(())
}

// Simple version without location constraint
async fn create_bucket_simple(
    client: &Client,
    bucket_name: &str
) -> Result<(), Box<dyn std::error::Error>> {
    match client.create_bucket().bucket(bucket_name).send().await {
        Ok(_) => {
            println!("Bucket '{}' created successfully", bucket_name);
            Ok(())
        }
        Err(e) => {
            if e.to_string().contains("BucketAlreadyExists") {
                println!("Bucket '{}' already exists", bucket_name);
                Ok(())
            } else {
                Err(e.into())
            }
        }
    }
}
```

### Upload Object

```rust
use aws_sdk_s3::primitives::ByteStream;
use std::path::Path;

async fn upload_file(
    client: &Client,
    bucket: &str,
    key: &str,
    file_path: &Path
) -> Result<(), Box<dyn std::error::Error>> {
    let body = ByteStream::from_path(file_path).await?;

    let resp = client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(body)
        .content_type("application/octet-stream")
        .metadata("uploaded-by", "rust-app")
        .metadata("upload-date", chrono::Utc::now().to_rfc3339())
        .send()
        .await?;

    println!("File uploaded successfully. ETag: {:?}", resp.e_tag());
    Ok(())
}

// Upload from bytes
async fn upload_bytes(
    client: &Client,
    bucket: &str,
    key: &str,
    data: Vec<u8>
) -> Result<(), Box<dyn std::error::Error>> {
    let body = ByteStream::from(data);

    let resp = client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(body)
        .send()
        .await?;

    println!("Data uploaded successfully. ETag: {:?}", resp.e_tag());
    Ok(())
}
```

### Download Object

```rust
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

async fn download_file(
    client: &Client,
    bucket: &str,
    key: &str,
    file_path: &Path
) -> Result<(), Box<dyn std::error::Error>> {
    let resp = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await?;

    let data = resp.body.collect().await?;
    let bytes = data.into_bytes();

    let mut file = File::create(file_path).await?;
    file.write_all(&bytes).await?;

    println!("File downloaded to {:?}", file_path);
    Ok(())
}

// Download to memory
async fn download_to_memory(
    client: &Client,
    bucket: &str,
    key: &str
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let resp = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await?;

    let data = resp.body.collect().await?;
    Ok(data.into_bytes().to_vec())
}
```

### List Objects

```rust
async fn list_objects(
    client: &Client,
    bucket: &str,
    prefix: Option<&str>
) -> Result<(), Box<dyn std::error::Error>> {
    let mut request = client.list_objects_v2().bucket(bucket);

    if let Some(p) = prefix {
        request = request.prefix(p);
    }

    let resp = request.max_keys(1000).send().await?;

    println!("Objects in {}:", bucket);
    if let Some(contents) = resp.contents() {
        for object in contents {
            println!("  - {} (Size: {}, Modified: {:?})",
                object.key().unwrap_or("Unknown"),
                object.size().unwrap_or(0),
                object.last_modified()
            );
        }
    }

    Ok(())
}

// List all objects with pagination
async fn list_all_objects(
    client: &Client,
    bucket: &str,
    prefix: Option<&str>
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut objects = Vec::new();
    let mut continuation_token: Option<String> = None;

    loop {
        let mut request = client.list_objects_v2().bucket(bucket);

        if let Some(p) = prefix {
            request = request.prefix(p);
        }

        if let Some(token) = continuation_token {
            request = request.continuation_token(token);
        }

        let resp = request.max_keys(1000).send().await?;

        if let Some(contents) = resp.contents() {
            for object in contents {
                if let Some(key) = object.key() {
                    objects.push(key.to_string());
                }
            }
        }

        if !resp.is_truncated().unwrap_or(false) {
            break;
        }

        continuation_token = resp.next_continuation_token().map(|s| s.to_string());
    }

    println!("Total objects: {}", objects.len());
    Ok(objects)
}
```

### Delete Object

```rust
async fn delete_object(
    client: &Client,
    bucket: &str,
    key: &str
) -> Result<(), Box<dyn std::error::Error>> {
    client
        .delete_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await?;

    println!("Object '{}' deleted successfully", key);
    Ok(())
}

// Delete multiple objects
use aws_sdk_s3::types::{Delete, ObjectIdentifier};

async fn delete_multiple_objects(
    client: &Client,
    bucket: &str,
    keys: Vec<String>
) -> Result<(), Box<dyn std::error::Error>> {
    let objects: Vec<ObjectIdentifier> = keys
        .iter()
        .map(|key| {
            ObjectIdentifier::builder()
                .key(key)
                .build()
                .unwrap()
        })
        .collect();

    let delete = Delete::builder()
        .set_objects(Some(objects))
        .quiet(false)
        .build()?;

    let resp = client
        .delete_objects()
        .bucket(bucket)
        .delete(delete)
        .send()
        .await?;

    if let Some(deleted) = resp.deleted() {
        println!("Deleted objects:");
        for obj in deleted {
            println!("  - {}", obj.key().unwrap_or("Unknown"));
        }
    }

    if let Some(errors) = resp.errors() {
        println!("Errors:");
        for err in errors {
            println!("  - {}: {}",
                err.key().unwrap_or("Unknown"),
                err.message().unwrap_or("Unknown error")
            );
        }
    }

    Ok(())
}
```

## Advanced Operations

### Get Object Metadata

```rust
async fn get_object_metadata(
    client: &Client,
    bucket: &str,
    key: &str
) -> Result<(), Box<dyn std::error::Error>> {
    let resp = client
        .head_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await?;

    println!("Object Metadata:");
    println!("  ContentType: {:?}", resp.content_type());
    println!("  ContentLength: {:?}", resp.content_length());
    println!("  LastModified: {:?}", resp.last_modified());
    println!("  ETag: {:?}", resp.e_tag());

    if let Some(metadata) = resp.metadata() {
        println!("  Metadata:");
        for (key, value) in metadata {
            println!("    {}: {}", key, value);
        }
    }

    Ok(())
}
```

### Copy Object

```rust
async fn copy_object(
    client: &Client,
    source_bucket: &str,
    source_key: &str,
    dest_bucket: &str,
    dest_key: &str
) -> Result<(), Box<dyn std::error::Error>> {
    let copy_source = format!("{}/{}", source_bucket, source_key);

    let resp = client
        .copy_object()
        .copy_source(copy_source)
        .bucket(dest_bucket)
        .key(dest_key)
        .metadata_directive(aws_sdk_s3::types::MetadataDirective::Copy)
        .send()
        .await?;

    if let Some(copy_result) = resp.copy_object_result() {
        println!("Object copied successfully. ETag: {:?}", copy_result.e_tag());
    }

    Ok(())
}
```

### Generate Presigned URLs

```rust
use aws_sdk_s3::presigning::PresigningConfig;
use std::time::Duration;

async fn generate_presigned_get_url(
    client: &Client,
    bucket: &str,
    key: &str,
    expires_in: Duration
) -> Result<String, Box<dyn std::error::Error>> {
    let presigning_config = PresigningConfig::builder()
        .expires_in(expires_in)
        .build()?;

    let presigned = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .presigned(presigning_config)
        .await?;

    println!("Presigned GET URL: {}", presigned.uri());
    Ok(presigned.uri().to_string())
}

async fn generate_presigned_put_url(
    client: &Client,
    bucket: &str,
    key: &str,
    expires_in: Duration
) -> Result<String, Box<dyn std::error::Error>> {
    let presigning_config = PresigningConfig::builder()
        .expires_in(expires_in)
        .build()?;

    let presigned = client
        .put_object()
        .bucket(bucket)
        .key(key)
        .presigned(presigning_config)
        .await?;

    println!("Presigned PUT URL: {}", presigned.uri());
    Ok(presigned.uri().to_string())
}
```

## Multipart Upload

### Basic Multipart Upload

```rust
use aws_sdk_s3::types::CompletedPart;
use bytes::Bytes;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, BufReader};

async fn multipart_upload(
    client: &Client,
    bucket: &str,
    key: &str,
    file_path: &Path
) -> Result<(), Box<dyn std::error::Error>> {
    const CHUNK_SIZE: usize = 5 * 1024 * 1024; // 5MB

    // Initiate multipart upload
    let multipart_upload = client
        .create_multipart_upload()
        .bucket(bucket)
        .key(key)
        .send()
        .await?;

    let upload_id = multipart_upload
        .upload_id()
        .ok_or("No upload ID returned")?;

    println!("Multipart upload initiated. UploadId: {}", upload_id);

    let mut file = File::open(file_path).await?;
    let mut buffer = vec![0; CHUNK_SIZE];
    let mut part_number = 1;
    let mut completed_parts = Vec::new();

    loop {
        let bytes_read = file.read(&mut buffer).await?;
        if bytes_read == 0 {
            break;
        }

        let body = ByteStream::from(Bytes::from(buffer[..bytes_read].to_vec()));

        // Upload part
        let upload_part_resp = client
            .upload_part()
            .bucket(bucket)
            .key(key)
            .upload_id(upload_id)
            .part_number(part_number)
            .body(body)
            .send()
            .await?;

        let completed_part = CompletedPart::builder()
            .e_tag(upload_part_resp.e_tag().unwrap_or_default())
            .part_number(part_number)
            .build();

        completed_parts.push(completed_part);
        println!("Part {} uploaded", part_number);

        part_number += 1;
    }

    // Complete multipart upload
    let completed_multipart_upload = aws_sdk_s3::types::CompletedMultipartUpload::builder()
        .set_parts(Some(completed_parts))
        .build();

    let complete_resp = client
        .complete_multipart_upload()
        .bucket(bucket)
        .key(key)
        .upload_id(upload_id)
        .multipart_upload(completed_multipart_upload)
        .send()
        .await?;

    println!("Multipart upload completed. Location: {:?}", complete_resp.location());
    Ok(())
}
```

### Multipart Upload with Error Handling

```rust
async fn multipart_upload_with_abort(
    client: &Client,
    bucket: &str,
    key: &str,
    file_path: &Path
) -> Result<(), Box<dyn std::error::Error>> {
    let multipart_upload = client
        .create_multipart_upload()
        .bucket(bucket)
        .key(key)
        .send()
        .await?;

    let upload_id = multipart_upload
        .upload_id()
        .ok_or("No upload ID returned")?
        .to_string();

    // Wrap the upload in a function that handles abort on error
    match upload_parts(client, bucket, key, &upload_id, file_path).await {
        Ok(completed_parts) => {
            complete_upload(client, bucket, key, &upload_id, completed_parts).await
        }
        Err(e) => {
            // Abort the upload on error
            println!("Upload failed, aborting multipart upload");
            client
                .abort_multipart_upload()
                .bucket(bucket)
                .key(key)
                .upload_id(&upload_id)
                .send()
                .await?;
            Err(e)
        }
    }
}

async fn upload_parts(
    client: &Client,
    bucket: &str,
    key: &str,
    upload_id: &str,
    file_path: &Path
) -> Result<Vec<CompletedPart>, Box<dyn std::error::Error>> {
    // Implementation similar to above
    // Returns completed parts or error
    Ok(vec![])
}

async fn complete_upload(
    client: &Client,
    bucket: &str,
    key: &str,
    upload_id: &str,
    parts: Vec<CompletedPart>
) -> Result<(), Box<dyn std::error::Error>> {
    // Complete the multipart upload
    Ok(())
}
```

## Error Handling

### Custom Error Type

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum S3Error {
    #[error("Bucket not found: {0}")]
    BucketNotFound(String),

    #[error("Object not found: {0}")]
    ObjectNotFound(String),

    #[error("Access denied")]
    AccessDenied,

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("SDK error: {0}")]
    SdkError(#[from] aws_sdk_s3::Error),

    #[error("Other error: {0}")]
    Other(String),
}

// Convert SDK errors to custom errors
impl From<aws_sdk_s3::error::SdkError<aws_sdk_s3::operation::get_object::GetObjectError>> for S3Error {
    fn from(err: aws_sdk_s3::error::SdkError<aws_sdk_s3::operation::get_object::GetObjectError>) -> Self {
        match err {
            aws_sdk_s3::error::SdkError::ServiceError(service_err) => {
                match service_err.err() {
                    aws_sdk_s3::operation::get_object::GetObjectError::NoSuchKey(_) => {
                        S3Error::ObjectNotFound("Object not found".to_string())
                    }
                    _ => S3Error::Other(format!("{:?}", service_err))
                }
            }
            _ => S3Error::Other(format!("{:?}", err))
        }
    }
}
```

### Retry Logic

```rust
use std::time::Duration;
use tokio::time::sleep;

async fn retry_operation<F, T, E>(
    mut operation: F,
    max_retries: u32,
    initial_delay: Duration
) -> Result<T, E>
where
    F: FnMut() -> futures::future::BoxFuture<'static, Result<T, E>>,
    E: std::fmt::Display,
{
    let mut delay = initial_delay;

    for attempt in 0..max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) if attempt < max_retries - 1 => {
                println!("Attempt {} failed: {}. Retrying in {:?}...",
                    attempt + 1, e, delay);
                sleep(delay).await;
                delay *= 2; // Exponential backoff
            }
            Err(e) => return Err(e),
        }
    }

    unreachable!()
}

// Usage
let result = retry_operation(
    || Box::pin(upload_file(&client, "bucket", "key", &path)),
    3,
    Duration::from_secs(1)
).await?;
```

## Async Operations

### Concurrent Operations

```rust
use futures::future::join_all;
use std::sync::Arc;

async fn batch_upload(
    client: Arc<Client>,
    bucket: &str,
    files: Vec<(String, PathBuf)>
) -> Result<Vec<Result<(), Box<dyn std::error::Error + Send + Sync>>>, Box<dyn std::error::Error>> {
    let mut tasks = Vec::new();

    for (key, path) in files {
        let client = client.clone();
        let bucket = bucket.to_string();

        let task = tokio::spawn(async move {
            upload_file_helper(client, &bucket, &key, &path).await
        });

        tasks.push(task);
    }

    let results = join_all(tasks).await;

    // Convert JoinHandle results
    let upload_results: Vec<_> = results
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    let successful = upload_results.iter().filter(|r| r.is_ok()).count();
    let failed = upload_results.len() - successful;

    println!("Batch upload: {} successful, {} failed", successful, failed);

    Ok(upload_results)
}

async fn upload_file_helper(
    client: Arc<Client>,
    bucket: &str,
    key: &str,
    path: &Path
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let body = ByteStream::from_path(path).await?;

    client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(body)
        .send()
        .await?;

    println!("Uploaded: {}", key);
    Ok(())
}
```

### Stream Processing

```rust
use tokio_stream::StreamExt;
use futures::stream;

async fn process_objects_stream(
    client: &Client,
    bucket: &str,
    prefix: &str
) -> Result<(), Box<dyn std::error::Error>> {
    // Create a stream of objects
    let objects = list_all_objects(client, bucket, Some(prefix)).await?;

    let mut stream = stream::iter(objects)
        .map(|key| async move {
            // Process each object
            process_object(client, bucket, &key).await
        })
        .buffer_unordered(10); // Process 10 concurrently

    while let Some(result) = stream.next().await {
        match result.await {
            Ok(_) => println!("Processed successfully"),
            Err(e) => println!("Processing failed: {}", e),
        }
    }

    Ok(())
}

async fn process_object(
    client: &Client,
    bucket: &str,
    key: &str
) -> Result<(), Box<dyn std::error::Error>> {
    // Your processing logic here
    println!("Processing: {}", key);
    Ok(())
}
```

## Best Practices

### 1. Connection Pooling

```rust
use aws_sdk_s3::config::timeout::TimeoutConfig;
use aws_sdk_s3::config::retry::RetryConfig;
use std::time::Duration;

fn create_optimized_client(credentials: Credentials) -> Client {
    let timeout_config = TimeoutConfig::builder()
        .operation_timeout(Duration::from_secs(30))
        .operation_attempt_timeout(Duration::from_secs(10))
        .build();

    let retry_config = RetryConfig::standard()
        .with_max_attempts(3);

    let config = Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .credentials_provider(SharedCredentialsProvider::new(credentials))
        .timeout_config(timeout_config)
        .retry_config(retry_config)
        .endpoint_url("http://localhost:9000")
        .force_path_style(true)
        .build();

    Client::from_conf(config)
}
```

### 2. Resource Management

```rust
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct S3Manager {
    client: Arc<Client>,
    bucket: String,
    cache: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl S3Manager {
    pub fn new(client: Client, bucket: String) -> Self {
        Self {
            client: Arc::new(client),
            bucket,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get_cached(
        &self,
        key: &str
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(data) = cache.get(key) {
                return Ok(data.clone());
            }
        }

        // Download from S3
        let data = download_to_memory(&self.client, &self.bucket, key).await?;

        // Update cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(key.to_string(), data.clone());
        }

        Ok(data)
    }
}
```

### 3. Progress Tracking

```rust
use indicatif::{ProgressBar, ProgressStyle};

async fn upload_with_progress(
    client: &Client,
    bucket: &str,
    key: &str,
    file_path: &Path
) -> Result<(), Box<dyn std::error::Error>> {
    let file_size = std::fs::metadata(file_path)?.len();

    let pb = ProgressBar::new(file_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {bytes}/{total_bytes} ({eta})")?
            .progress_chars("##-")
    );

    // Read file in chunks and update progress
    let mut file = File::open(file_path).await?;
    let mut buffer = Vec::new();
    let mut total_read = 0u64;

    loop {
        let mut chunk = vec![0; 8192];
        let n = file.read(&mut chunk).await?;

        if n == 0 {
            break;
        }

        chunk.truncate(n);
        buffer.extend_from_slice(&chunk);
        total_read += n as u64;
        pb.set_position(total_read);
    }

    pb.finish_with_message("Upload complete");

    // Upload the complete buffer
    let body = ByteStream::from(buffer);
    client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(body)
        .send()
        .await?;

    Ok(())
}
```

## Complete Examples

### Example 1: S3 File Sync Tool

```rust
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use sha2::{Sha256, Digest};

pub struct S3Sync {
    client: Client,
    bucket: String,
}

impl S3Sync {
    pub fn new(client: Client, bucket: String) -> Self {
        Self { client, bucket }
    }

    pub async fn sync_directory(
        &self,
        local_dir: &Path,
        s3_prefix: &str
    ) -> Result<(), Box<dyn std::error::Error>> {
        let local_files = self.scan_local_directory(local_dir)?;
        let s3_objects = self.list_s3_objects(s3_prefix).await?;

        let (to_upload, to_delete) = self.compare_files(
            &local_files,
            &s3_objects,
            local_dir,
            s3_prefix
        );

        // Upload new/modified files
        for (local_path, s3_key) in to_upload {
            println!("Uploading: {} -> {}", local_path.display(), s3_key);
            self.upload_file(&local_path, &s3_key).await?;
        }

        // Delete removed files
        if !to_delete.is_empty() {
            println!("Deleting {} objects", to_delete.len());
            delete_multiple_objects(&self.client, &self.bucket, to_delete).await?;
        }

        println!("Sync complete!");
        Ok(())
    }

    fn scan_local_directory(
        &self,
        dir: &Path
    ) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
        let mut files = HashMap::new();

        for entry in WalkDir::new(dir) {
            let entry = entry?;
            if entry.file_type().is_file() {
                let path = entry.path();
                let relative_path = path.strip_prefix(dir)?;
                let hash = self.calculate_file_hash(path)?;

                files.insert(
                    relative_path.to_string_lossy().to_string(),
                    hash
                );
            }
        }

        Ok(files)
    }

    fn calculate_file_hash(&self, path: &Path) -> Result<String, std::io::Error> {
        let mut file = std::fs::File::open(path)?;
        let mut hasher = Sha256::new();
        std::io::copy(&mut file, &mut hasher)?;
        Ok(format!("{:x}", hasher.finalize()))
    }

    async fn list_s3_objects(
        &self,
        prefix: &str
    ) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
        let mut objects = HashMap::new();
        let all_objects = list_all_objects(&self.client, &self.bucket, Some(prefix)).await?;

        for key in all_objects {
            // Get object metadata for ETag
            let resp = self.client
                .head_object()
                .bucket(&self.bucket)
                .key(&key)
                .send()
                .await?;

            if let Some(etag) = resp.e_tag() {
                let relative_key = key.strip_prefix(prefix)
                    .unwrap_or(&key)
                    .trim_start_matches('/');
                objects.insert(relative_key.to_string(), etag.trim_matches('"').to_string());
            }
        }

        Ok(objects)
    }

    fn compare_files(
        &self,
        local: &HashMap<String, String>,
        remote: &HashMap<String, String>,
        local_dir: &Path,
        s3_prefix: &str
    ) -> (Vec<(PathBuf, String)>, Vec<String>) {
        let mut to_upload = Vec::new();
        let mut to_delete = Vec::new();

        // Find files to upload
        for (local_path, local_hash) in local {
            let s3_key = format!("{}/{}", s3_prefix, local_path);

            if !remote.contains_key(local_path) ||
               remote.get(local_path) != Some(local_hash) {
                let full_path = local_dir.join(local_path);
                to_upload.push((full_path, s3_key));
            }
        }

        // Find objects to delete
        for remote_path in remote.keys() {
            if !local.contains_key(remote_path) {
                let s3_key = format!("{}/{}", s3_prefix, remote_path);
                to_delete.push(s3_key);
            }
        }

        (to_upload, to_delete)
    }

    async fn upload_file(
        &self,
        local_path: &Path,
        s3_key: &str
    ) -> Result<(), Box<dyn std::error::Error>> {
        let body = ByteStream::from_path(local_path).await?;

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(s3_key)
            .body(body)
            .send()
            .await?;

        Ok(())
    }
}

// Usage
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = create_client_from_env().await?;
    let sync = S3Sync::new(client, "my-bucket".to_string());

    sync.sync_directory(Path::new("./local-folder"), "remote-folder").await?;

    Ok(())
}
```

### Example 2: S3-Backed Cache

```rust
use serde::{Serialize, Deserialize};
use std::time::{SystemTime, Duration};

#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry<T> {
    data: T,
    expiry: SystemTime,
}

pub struct S3Cache {
    client: Client,
    bucket: String,
    default_ttl: Duration,
}

impl S3Cache {
    pub fn new(client: Client, bucket: String, default_ttl: Duration) -> Self {
        Self {
            client,
            bucket,
            default_ttl,
        }
    }

    pub async fn get<T>(&self, key: &str) -> Result<Option<T>, Box<dyn std::error::Error>>
    where
        T: for<'de> Deserialize<'de>,
    {
        // Try to get from S3
        match self.client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(resp) => {
                let data = resp.body.collect().await?;
                let bytes = data.into_bytes();

                let entry: CacheEntry<T> = serde_json::from_slice(&bytes)?;

                // Check if expired
                if entry.expiry > SystemTime::now() {
                    Ok(Some(entry.data))
                } else {
                    // Delete expired entry
                    self.delete(key).await?;
                    Ok(None)
                }
            }
            Err(_) => Ok(None),
        }
    }

    pub async fn set<T>(
        &self,
        key: &str,
        value: T,
        ttl: Option<Duration>
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        T: Serialize,
    {
        let ttl = ttl.unwrap_or(self.default_ttl);
        let entry = CacheEntry {
            data: value,
            expiry: SystemTime::now() + ttl,
        };

        let json = serde_json::to_vec(&entry)?;
        let body = ByteStream::from(json);

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(body)
            .content_type("application/json")
            .send()
            .await?;

        Ok(())
    }

    pub async fn delete(&self, key: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await?;

        Ok(())
    }
}

// Usage
async fn cache_example() -> Result<(), Box<dyn std::error::Error>> {
    let client = create_client_from_env().await?;
    let cache = S3Cache::new(
        client,
        "cache-bucket".to_string(),
        Duration::from_secs(3600)
    );

    // Set value
    cache.set("user:123", "John Doe", None).await?;

    // Get value
    if let Some(name): Option<String> = cache.get("user:123").await? {
        println!("Cached value: {}", name);
    }

    Ok(())
}
```

## Testing

### Unit Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use mockall::mock;

    mock! {
        S3Client {}

        #[async_trait]
        impl S3Operations for S3Client {
            async fn upload(&self, key: &str, data: Vec<u8>) -> Result<(), Error>;
            async fn download(&self, key: &str) -> Result<Vec<u8>, Error>;
        }
    }

    #[tokio::test]
    async fn test_upload() {
        let mut mock_client = MockS3Client::new();

        mock_client
            .expect_upload()
            .with(eq("test-key"), eq(vec![1, 2, 3]))
            .times(1)
            .returning(|_, _| Ok(()));

        let result = mock_client.upload("test-key", vec![1, 2, 3]).await;
        assert!(result.is_ok());
    }
}
```

### Integration Testing

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    use uuid::Uuid;

    async fn setup_test_client() -> Client {
        create_client_from_env().await.expect("Failed to create client")
    }

    #[tokio::test]
    async fn test_full_cycle() {
        let client = setup_test_client().await;
        let bucket = format!("test-bucket-{}", Uuid::new_v4());
        let key = "test-object";
        let data = b"Hello, IronBucket!";

        // Create bucket
        create_bucket_simple(&client, &bucket).await.unwrap();

        // Upload object
        upload_bytes(&client, &bucket, key, data.to_vec()).await.unwrap();

        // Download object
        let downloaded = download_to_memory(&client, &bucket, key).await.unwrap();
        assert_eq!(downloaded, data);

        // Delete object
        delete_object(&client, &bucket, key).await.unwrap();

        // Delete bucket
        client.delete_bucket().bucket(&bucket).send().await.unwrap();
    }
}
```

## Performance Tips

1. **Use async/await properly** - Don't block the runtime
2. **Leverage connection pooling** - Reuse HTTP connections
3. **Stream large files** - Avoid loading entire files into memory
4. **Use multipart upload** for files > 100MB
5. **Implement retry logic** with exponential backoff
6. **Use concurrent operations** with tokio::spawn or futures
7. **Cache frequently accessed objects** to reduce S3 calls
8. **Compress data** before uploading when appropriate

## Troubleshooting

### Common Issues

1. **Connection Refused**
   - Check endpoint URL and port
   - Verify IronBucket is running
   - Check firewall settings

2. **Invalid Credentials**
   - Verify access key and secret key
   - Check credential provider chain
   - Ensure region is set

3. **Slow Performance**
   - Enable connection keep-alive
   - Use multipart upload for large files
   - Increase concurrent operations
   - Check network latency

4. **Memory Issues**
   - Use streaming for large files
   - Process data in chunks
   - Implement proper cleanup
   - Monitor memory usage

## Additional Resources

- [AWS SDK for Rust Documentation](https://docs.rs/aws-sdk-s3/latest/aws_sdk_s3/)
- [S3 API Reference](https://docs.aws.amazon.com/AmazonS3/latest/API/)
- [IronBucket API Documentation](./API.md)
- [IronBucket Configuration Guide](../README.md#configuration)