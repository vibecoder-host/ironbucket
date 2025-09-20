# AWS CLI Usage Guide

This guide covers using the AWS CLI with IronBucket for S3 operations.

## Table of Contents

- [Setup](#setup)
- [Basic Operations](#basic-operations)
- [Advanced Operations](#advanced-operations)
- [Multipart Uploads](#multipart-uploads)
- [Versioning](#versioning)
- [Encryption](#encryption)
- [CORS & Policies](#cors--policies)
- [Lifecycle Management](#lifecycle-management)
- [Performance Tips](#performance-tips)
- [Troubleshooting](#troubleshooting)

---

## Setup

### Install AWS CLI

```bash
# macOS
brew install awscli

# Linux
curl "https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip" -o "awscliv2.zip"
unzip awscliv2.zip
sudo ./aws/install

# Windows
# Download installer from https://aws.amazon.com/cli/
```

### Configure Credentials

#### Method 1: Environment Variables (Recommended)

```bash
# Set your IronBucket credentials
export IRONBUCKET_ACCESS_KEY="your-access-key"
export IRONBUCKET_SECRET_KEY="your-secret-key"
export IRONBUCKET_ENDPOINT="http://172.17.0.1:20000"

# Configure AWS CLI to use these credentials
export AWS_ACCESS_KEY_ID="${IRONBUCKET_ACCESS_KEY}"
export AWS_SECRET_ACCESS_KEY="${IRONBUCKET_SECRET_KEY}"
export AWS_DEFAULT_REGION=us-east-1
export AWS_ENDPOINT="${IRONBUCKET_ENDPOINT}"
```

#### Method 2: AWS Configure

```bash
aws configure
# AWS Access Key ID: [enter your access key]
# AWS Secret Access Key: [enter your secret key]
# Default region name: us-east-1
# Default output format: json
```

#### Method 3: Profile Configuration

```bash
# Create a profile for IronBucket
aws configure --profile ironbucket
```

Edit `~/.aws/config`:
```ini
[profile ironbucket]
region = us-east-1
endpoint_url = http://172.17.0.1:20000
```

Edit `~/.aws/credentials`:
```ini
[ironbucket]
aws_access_key_id = YOUR_ACCESS_KEY
aws_secret_access_key = YOUR_SECRET_KEY
```

Use profile:
```bash
aws --profile ironbucket s3 ls
```

---

## Basic Operations

### Bucket Management

```bash
# List all buckets
aws --endpoint-url $AWS_ENDPOINT s3 ls

# Create a bucket
aws --endpoint-url $AWS_ENDPOINT s3 mb s3://my-bucket

# Remove an empty bucket
aws --endpoint-url $AWS_ENDPOINT s3 rb s3://my-bucket

# Remove bucket and all contents
aws --endpoint-url $AWS_ENDPOINT s3 rb s3://my-bucket --force
```

### Object Operations

```bash
# Upload a file
aws --endpoint-url $AWS_ENDPOINT s3 cp file.txt s3://my-bucket/

# Upload with custom metadata
aws --endpoint-url $AWS_ENDPOINT s3 cp file.txt s3://my-bucket/ \
    --metadata key1=value1,key2=value2

# Upload with storage class
aws --endpoint-url $AWS_ENDPOINT s3 cp file.txt s3://my-bucket/ \
    --storage-class STANDARD

# Download a file
aws --endpoint-url $AWS_ENDPOINT s3 cp s3://my-bucket/file.txt ./

# Copy between buckets
aws --endpoint-url $AWS_ENDPOINT s3 cp \
    s3://source-bucket/file.txt \
    s3://dest-bucket/file.txt

# Move a file
aws --endpoint-url $AWS_ENDPOINT s3 mv \
    s3://my-bucket/old-name.txt \
    s3://my-bucket/new-name.txt

# Delete a file
aws --endpoint-url $AWS_ENDPOINT s3 rm s3://my-bucket/file.txt

# Delete all files with prefix
aws --endpoint-url $AWS_ENDPOINT s3 rm s3://my-bucket/logs/ --recursive
```

### Listing Objects

```bash
# List objects in bucket
aws --endpoint-url $AWS_ENDPOINT s3 ls s3://my-bucket/

# List with prefix
aws --endpoint-url $AWS_ENDPOINT s3 ls s3://my-bucket/photos/

# Recursive listing
aws --endpoint-url $AWS_ENDPOINT s3 ls s3://my-bucket/ --recursive

# Human-readable sizes
aws --endpoint-url $AWS_ENDPOINT s3 ls s3://my-bucket/ --human-readable

# Summarize total size
aws --endpoint-url $AWS_ENDPOINT s3 ls s3://my-bucket/ \
    --recursive --summarize
```

### Sync Operations

```bash
# Sync local directory to bucket
aws --endpoint-url $AWS_ENDPOINT s3 sync ./local-dir s3://my-bucket/

# Sync bucket to local directory
aws --endpoint-url $AWS_ENDPOINT s3 sync s3://my-bucket/ ./local-dir

# Sync with delete (mirror)
aws --endpoint-url $AWS_ENDPOINT s3 sync ./local-dir s3://my-bucket/ --delete

# Sync only specific files
aws --endpoint-url $AWS_ENDPOINT s3 sync ./local-dir s3://my-bucket/ \
    --exclude "*" --include "*.jpg"

# Sync with size-only comparison
aws --endpoint-url $AWS_ENDPOINT s3 sync ./local-dir s3://my-bucket/ \
    --size-only
```

---

## Advanced Operations

### S3API Commands

The `s3api` commands provide lower-level access to S3 operations:

```bash
# Get object with metadata
aws --endpoint-url $AWS_ENDPOINT s3api get-object \
    --bucket my-bucket \
    --key file.txt \
    output.txt

# Head object (metadata only)
aws --endpoint-url $AWS_ENDPOINT s3api head-object \
    --bucket my-bucket \
    --key file.txt

# Put object with all options
aws --endpoint-url $AWS_ENDPOINT s3api put-object \
    --bucket my-bucket \
    --key file.txt \
    --body file.txt \
    --content-type text/plain \
    --metadata '{"custom-key":"custom-value"}' \
    --cache-control "max-age=3600" \
    --content-disposition "attachment; filename=download.txt"

# List objects v2 with pagination
aws --endpoint-url $AWS_ENDPOINT s3api list-objects-v2 \
    --bucket my-bucket \
    --prefix photos/ \
    --delimiter / \
    --max-keys 100

# Get object with range
aws --endpoint-url $AWS_ENDPOINT s3api get-object \
    --bucket my-bucket \
    --key large-file.bin \
    --range "bytes=0-1023" \
    partial.bin
```

### Batch Delete

```bash
# Delete multiple objects
aws --endpoint-url $AWS_ENDPOINT s3api delete-objects \
    --bucket my-bucket \
    --delete '{
        "Objects": [
            {"Key": "file1.txt"},
            {"Key": "file2.txt"},
            {"Key": "dir/file3.txt"}
        ],
        "Quiet": false
    }'

# Generate delete list from query
aws --endpoint-url $AWS_ENDPOINT s3api list-objects-v2 \
    --bucket my-bucket \
    --prefix old-logs/ \
    --query 'Contents[].{Key:Key}' \
    --output json | \
jq '{Objects: .}' | \
aws --endpoint-url $AWS_ENDPOINT s3api delete-objects \
    --bucket my-bucket \
    --delete file://dev/stdin
```

### Presigned URLs

```bash
# Generate presigned URL for download (1 hour)
aws --endpoint-url $AWS_ENDPOINT s3 presign \
    s3://my-bucket/file.txt \
    --expires-in 3600

# Generate presigned URL for upload
aws --endpoint-url $AWS_ENDPOINT s3 presign \
    s3://my-bucket/new-file.txt \
    --expires-in 3600 \
    --region us-east-1
```

---

## Multipart Uploads

### Automatic Multipart Upload

AWS CLI automatically uses multipart upload for large files:

```bash
# Upload large file (auto multipart for files > 8MB)
aws --endpoint-url $AWS_ENDPOINT s3 cp large-file.zip s3://my-bucket/

# Configure multipart threshold and chunk size
aws --endpoint-url $AWS_ENDPOINT configure set s3.multipart_threshold 64MB
aws --endpoint-url $AWS_ENDPOINT configure set s3.multipart_chunksize 16MB
aws --endpoint-url $AWS_ENDPOINT configure set s3.max_concurrent_requests 10
```

### Manual Multipart Upload

```bash
# Initiate multipart upload
UPLOAD_ID=$(aws --endpoint-url $AWS_ENDPOINT s3api create-multipart-upload \
    --bucket my-bucket \
    --key large-file.zip \
    --content-type application/zip \
    --query 'UploadId' \
    --output text)

# Upload parts
aws --endpoint-url $AWS_ENDPOINT s3api upload-part \
    --bucket my-bucket \
    --key large-file.zip \
    --part-number 1 \
    --body part1.bin \
    --upload-id $UPLOAD_ID

# List parts
aws --endpoint-url $AWS_ENDPOINT s3api list-parts \
    --bucket my-bucket \
    --key large-file.zip \
    --upload-id $UPLOAD_ID

# Complete upload
aws --endpoint-url $AWS_ENDPOINT s3api complete-multipart-upload \
    --bucket my-bucket \
    --key large-file.zip \
    --upload-id $UPLOAD_ID \
    --multipart-upload '{
        "Parts": [
            {
                "ETag": "\"etag1\"",
                "PartNumber": 1
            },
            {
                "ETag": "\"etag2\"",
                "PartNumber": 2
            }
        ]
    }'

# Abort upload
aws --endpoint-url $AWS_ENDPOINT s3api abort-multipart-upload \
    --bucket my-bucket \
    --key large-file.zip \
    --upload-id $UPLOAD_ID

# List all multipart uploads
aws --endpoint-url $AWS_ENDPOINT s3api list-multipart-uploads \
    --bucket my-bucket
```

---

## Versioning

### Enable/Disable Versioning

```bash
# Enable versioning
aws --endpoint-url $AWS_ENDPOINT s3api put-bucket-versioning \
    --bucket my-bucket \
    --versioning-configuration Status=Enabled

# Suspend versioning
aws --endpoint-url $AWS_ENDPOINT s3api put-bucket-versioning \
    --bucket my-bucket \
    --versioning-configuration Status=Suspended

# Check versioning status
aws --endpoint-url $AWS_ENDPOINT s3api get-bucket-versioning \
    --bucket my-bucket
```

### Working with Versions

```bash
# List all versions
aws --endpoint-url $AWS_ENDPOINT s3api list-object-versions \
    --bucket my-bucket

# List versions for specific key
aws --endpoint-url $AWS_ENDPOINT s3api list-object-versions \
    --bucket my-bucket \
    --prefix file.txt

# Download specific version
aws --endpoint-url $AWS_ENDPOINT s3api get-object \
    --bucket my-bucket \
    --key file.txt \
    --version-id v123456 \
    file-v123456.txt

# Delete specific version
aws --endpoint-url $AWS_ENDPOINT s3api delete-object \
    --bucket my-bucket \
    --key file.txt \
    --version-id v123456

# Restore previous version (copy it as latest)
aws --endpoint-url $AWS_ENDPOINT s3api copy-object \
    --bucket my-bucket \
    --key file.txt \
    --copy-source my-bucket/file.txt?versionId=v123456
```

---

## Encryption

### Server-Side Encryption

```bash
# Enable default encryption for bucket
aws --endpoint-url $AWS_ENDPOINT s3api put-bucket-encryption \
    --bucket my-bucket \
    --server-side-encryption-configuration '{
        "Rules": [
            {
                "ApplyServerSideEncryptionByDefault": {
                    "SSEAlgorithm": "AES256"
                }
            }
        ]
    }'

# Check encryption configuration
aws --endpoint-url $AWS_ENDPOINT s3api get-bucket-encryption \
    --bucket my-bucket

# Upload with encryption
aws --endpoint-url $AWS_ENDPOINT s3 cp file.txt s3://my-bucket/ \
    --sse AES256

# Upload with customer-provided key
aws --endpoint-url $AWS_ENDPOINT s3api put-object \
    --bucket my-bucket \
    --key secret.txt \
    --body secret.txt \
    --sse-customer-algorithm AES256 \
    --sse-customer-key $(echo -n 'my32characterslongpasswordhere!!' | base64) \
    --sse-customer-key-md5 $(echo -n 'my32characterslongpasswordhere!!' | openssl md5 -binary | base64)
```

---

## CORS & Policies

### CORS Configuration

```bash
# Set CORS policy
aws --endpoint-url $AWS_ENDPOINT s3api put-bucket-cors \
    --bucket my-bucket \
    --cors-configuration '{
        "CORSRules": [
            {
                "AllowedOrigins": ["https://example.com"],
                "AllowedMethods": ["GET", "PUT", "POST"],
                "AllowedHeaders": ["*"],
                "ExposeHeaders": ["ETag"],
                "MaxAgeSeconds": 3000
            }
        ]
    }'

# Get CORS configuration
aws --endpoint-url $AWS_ENDPOINT s3api get-bucket-cors \
    --bucket my-bucket

# Delete CORS configuration
aws --endpoint-url $AWS_ENDPOINT s3api delete-bucket-cors \
    --bucket my-bucket
```

### Bucket Policies

```bash
# Create policy file
cat > policy.json << 'EOF'
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Sid": "PublicReadGetObject",
            "Effect": "Allow",
            "Principal": "*",
            "Action": "s3:GetObject",
            "Resource": "arn:aws:s3:::my-bucket/public/*"
        }
    ]
}
EOF

# Apply bucket policy
aws --endpoint-url $AWS_ENDPOINT s3api put-bucket-policy \
    --bucket my-bucket \
    --policy file://policy.json

# Get bucket policy
aws --endpoint-url $AWS_ENDPOINT s3api get-bucket-policy \
    --bucket my-bucket \
    --query Policy \
    --output text | jq .

# Delete bucket policy
aws --endpoint-url $AWS_ENDPOINT s3api delete-bucket-policy \
    --bucket my-bucket
```

---

## Lifecycle Management

```bash
# Create lifecycle configuration
cat > lifecycle.json << 'EOF'
{
    "Rules": [
        {
            "ID": "delete-old-logs",
            "Status": "Enabled",
            "Filter": {
                "Prefix": "logs/"
            },
            "Expiration": {
                "Days": 30
            }
        },
        {
            "ID": "archive-old-data",
            "Status": "Enabled",
            "Filter": {
                "Prefix": "data/"
            },
            "Transitions": [
                {
                    "Days": 90,
                    "StorageClass": "GLACIER"
                }
            ]
        }
    ]
}
EOF

# Set lifecycle policy
aws --endpoint-url $AWS_ENDPOINT s3api put-bucket-lifecycle-configuration \
    --bucket my-bucket \
    --lifecycle-configuration file://lifecycle.json

# Get lifecycle configuration
aws --endpoint-url $AWS_ENDPOINT s3api get-bucket-lifecycle-configuration \
    --bucket my-bucket

# Delete lifecycle configuration
aws --endpoint-url $AWS_ENDPOINT s3api delete-bucket-lifecycle \
    --bucket my-bucket
```

---

## Performance Tips

### Parallel Uploads

```bash
# Upload directory with parallel transfers
aws --endpoint-url $AWS_ENDPOINT s3 sync ./large-dir s3://my-bucket/ \
    --cli-write-timeout 0 \
    --cli-read-timeout 0

# Configure parallelism
aws configure set max_concurrent_requests 20
aws configure set max_queue_size 10000
```

### Large File Optimization

```bash
# Configure for large files
aws configure set s3.multipart_threshold 128MB
aws configure set s3.multipart_chunksize 64MB
aws configure set s3.max_bandwidth 100MB/s
```

### Batch Operations Script

```bash
#!/bin/bash
# Parallel upload script

BUCKET="my-bucket"
LOCAL_DIR="./data"
THREADS=10

find "$LOCAL_DIR" -type f | \
    parallel -j $THREADS \
    "aws --endpoint-url $AWS_ENDPOINT s3 cp {} s3://$BUCKET/{/}"
```

---

## Troubleshooting

### Debug Mode

```bash
# Enable debug output
aws --endpoint-url $AWS_ENDPOINT s3 ls --debug

# Verbose output
aws --endpoint-url $AWS_ENDPOINT s3 cp file.txt s3://my-bucket/ --debug 2>&1 | grep -i error
```

### Common Issues

#### Connection Errors

```bash
# Test connectivity
aws --endpoint-url $AWS_ENDPOINT s3 ls

# Check endpoint
curl -I $AWS_ENDPOINT

# Verify credentials
aws --endpoint-url $AWS_ENDPOINT sts get-caller-identity
```

#### Signature Errors

```bash
# Ensure correct region
aws --endpoint-url $AWS_ENDPOINT s3 ls --region us-east-1

# Check time sync
date
ntpdate -q pool.ntp.org
```

#### Performance Issues

```bash
# Profile request
time aws --endpoint-url $AWS_ENDPOINT s3 cp large-file s3://my-bucket/

# Monitor bandwidth
aws --endpoint-url $AWS_ENDPOINT s3 cp large-file s3://my-bucket/ \
    --expected-size 1073741824 \
    --cli-write-timeout 0
```

### Useful Aliases

Add to `~/.bashrc` or `~/.zshrc`:

```bash
# IronBucket aliases
alias ibs3='aws --endpoint-url http://localhost:9000 s3'
alias ibs3api='aws --endpoint-url http://localhost:9000 s3api'

# Usage
ibs3 ls
ibs3 cp file.txt s3://my-bucket/
ibs3api get-bucket-versioning --bucket my-bucket
```

---

## Scripts and Automation

### Backup Script

```bash
#!/bin/bash
# Daily backup to IronBucket

BUCKET="backups"
SOURCE="/var/data"
DATE=$(date +%Y%m%d)
ENDPOINT="http://localhost:9000"

# Create daily backup
aws --endpoint-url $ENDPOINT s3 sync \
    $SOURCE \
    s3://$BUCKET/$DATE/ \
    --delete \
    --storage-class STANDARD

# Remove backups older than 30 days
aws --endpoint-url $ENDPOINT s3api list-objects-v2 \
    --bucket $BUCKET \
    --query "Contents[?LastModified<='$(date -d '30 days ago' --iso-8601)'].Key" \
    --output text | \
xargs -I {} aws --endpoint-url $ENDPOINT s3 rm s3://$BUCKET/{}
```

### Migration Script

```bash
#!/bin/bash
# Migrate from S3 to IronBucket

SOURCE_BUCKET="s3://aws-bucket"
DEST_ENDPOINT="http://localhost:9000"
DEST_BUCKET="s3://migrated-bucket"

# Create destination bucket
aws --endpoint-url $DEST_ENDPOINT s3 mb $DEST_BUCKET

# Copy all objects
aws s3 sync $SOURCE_BUCKET ./temp-migration/
aws --endpoint-url $DEST_ENDPOINT s3 sync ./temp-migration/ $DEST_BUCKET

# Verify
aws s3 ls $SOURCE_BUCKET --recursive --summarize > source.txt
aws --endpoint-url $DEST_ENDPOINT s3 ls $DEST_BUCKET --recursive --summarize > dest.txt
diff source.txt dest.txt
```

---

*For more examples in other programming languages, see:*
- [Node.js Usage Guide](USAGE_NODEJS.md)
- [Python Usage Guide](USAGE_PYTHON.md)
- [Rust Usage Guide](USAGE_RUST.md)