# IronBucket API Reference

Complete HTTP API documentation for IronBucket S3-compatible storage server.

## Table of Contents

- [Authentication](#authentication)
- [Common Headers](#common-headers)
- [Error Responses](#error-responses)
- [Bucket Operations](#bucket-operations)
- [Object Operations](#object-operations)
- [Multipart Upload](#multipart-upload)
- [Advanced Operations](#advanced-operations)

---

## Authentication

IronBucket uses AWS Signature Version 4 for request authentication.

### Required Headers

```http
Authorization: AWS4-HMAC-SHA256
    Credential=ACCESS_KEY/20250101/us-east-1/s3/aws4_request,
    SignedHeaders=host;x-amz-content-sha256;x-amz-date,
    Signature=SIGNATURE_HASH
x-amz-date: 20250101T000000Z
x-amz-content-sha256: PAYLOAD_HASH
```

### Signature Calculation

1. Create canonical request
2. Create string to sign
3. Calculate signature
4. Add to Authorization header

Example signature components:
- **Algorithm**: `AWS4-HMAC-SHA256`
- **Credential**: `ACCESS_KEY/DATE/REGION/SERVICE/aws4_request`
- **SignedHeaders**: List of signed headers (lowercase, sorted)
- **Signature**: Hex-encoded HMAC-SHA256

---

## Common Headers

### Request Headers

| Header | Description | Required |
|--------|-------------|----------|
| `Authorization` | AWS Signature v4 | Yes |
| `x-amz-date` | Request timestamp | Yes |
| `x-amz-content-sha256` | SHA256 of request body | Yes |
| `Content-Type` | Media type of body | For PUT |
| `Content-Length` | Size of request body | For PUT |
| `x-amz-meta-*` | Custom metadata | No |
| `x-amz-storage-class` | Storage class | No |
| `x-amz-server-side-encryption` | Encryption type | No |

### Response Headers

| Header | Description |
|--------|-------------|
| `ETag` | MD5 hash of object |
| `Last-Modified` | Last modification time |
| `Content-Length` | Size of response body |
| `Content-Type` | Media type |
| `x-amz-version-id` | Version ID (if versioning enabled) |
| `x-amz-delete-marker` | True if delete marker |
| `x-amz-meta-*` | Custom metadata |

---

## Error Responses

### Error Response Format

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Error>
    <Code>NoSuchBucket</Code>
    <Message>The specified bucket does not exist</Message>
    <Resource>/my-bucket</Resource>
    <RequestId>4442587FB7D0A2F9</RequestId>
</Error>
```

### Common Error Codes

| Code | HTTP Status | Description |
|------|-------------|-------------|
| `NoSuchBucket` | 404 | Bucket doesn't exist |
| `NoSuchKey` | 404 | Object doesn't exist |
| `BucketAlreadyExists` | 409 | Bucket name already in use |
| `BucketNotEmpty` | 409 | Bucket contains objects |
| `InvalidRequest` | 400 | Malformed request |
| `SignatureDoesNotMatch` | 403 | Authentication failed |
| `AccessDenied` | 403 | Permission denied |
| `RequestTimeout` | 408 | Request timed out |
| `EntityTooLarge` | 413 | Object exceeds max size |
| `InvalidRange` | 416 | Invalid byte range |
| `PreconditionFailed` | 412 | Precondition not met |
| `InternalError` | 500 | Server error |

---

## Bucket Operations

### List All Buckets

```http
GET /
```

**Response:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<ListAllMyBucketsResult>
    <Owner>
        <ID>ironbucket</ID>
        <DisplayName>IronBucket</DisplayName>
    </Owner>
    <Buckets>
        <Bucket>
            <Name>my-bucket</Name>
            <CreationDate>2025-01-01T00:00:00.000Z</CreationDate>
        </Bucket>
        <Bucket>
            <Name>another-bucket</Name>
            <CreationDate>2025-01-02T00:00:00.000Z</CreationDate>
        </Bucket>
    </Buckets>
</ListAllMyBucketsResult>
```

### Create Bucket

```http
PUT /{bucket}
```

**Optional Body (for region configuration):**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<CreateBucketConfiguration>
    <LocationConstraint>us-west-2</LocationConstraint>
</CreateBucketConfiguration>
```

**Response:** `200 OK` with empty body

### Delete Bucket

```http
DELETE /{bucket}
```

**Response:** `204 No Content`

**Errors:**
- `BucketNotEmpty` - Bucket contains objects
- `NoSuchBucket` - Bucket doesn't exist

### Head Bucket

```http
HEAD /{bucket}
```

**Response:**
- `200 OK` if bucket exists
- `404 Not Found` if bucket doesn't exist

### List Objects (v2)

```http
GET /{bucket}?list-type=2
```

**Query Parameters:**

| Parameter | Description | Default |
|-----------|-------------|---------|
| `list-type` | Must be `2` for v2 API | 1 |
| `prefix` | Filter by key prefix | None |
| `delimiter` | Group keys by delimiter | None |
| `max-keys` | Maximum keys to return | 1000 |
| `continuation-token` | Continue from previous response | None |
| `start-after` | Start listing after this key | None |
| `encoding-type` | Encoding for keys (`url`) | None |

**Response:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult>
    <Name>my-bucket</Name>
    <Prefix>photos/</Prefix>
    <Delimiter>/</Delimiter>
    <MaxKeys>1000</MaxKeys>
    <IsTruncated>false</IsTruncated>
    <NextContinuationToken>eyJrZXkiOiJwaG90b3MvMTAwLmpwZyJ9</NextContinuationToken>
    <KeyCount>3</KeyCount>

    <Contents>
        <Key>photos/001.jpg</Key>
        <LastModified>2025-01-01T00:00:00.000Z</LastModified>
        <ETag>"d41d8cd98f00b204e9800998ecf8427e"</ETag>
        <Size>1024576</Size>
        <StorageClass>STANDARD</StorageClass>
    </Contents>

    <CommonPrefixes>
        <Prefix>photos/2024/</Prefix>
    </CommonPrefixes>
</ListBucketResult>
```

### Get Bucket Location

```http
GET /{bucket}?location
```

**Response:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<LocationConstraint>us-east-1</LocationConstraint>
```

### Get Bucket Versioning

```http
GET /{bucket}?versioning
```

**Response:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<VersioningConfiguration>
    <Status>Enabled</Status>
</VersioningConfiguration>
```

### Put Bucket Versioning

```http
PUT /{bucket}?versioning
```

**Body:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<VersioningConfiguration>
    <Status>Enabled</Status>
</VersioningConfiguration>
```

### Get Bucket ACL

```http
GET /{bucket}?acl
```

**Response:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<AccessControlPolicy>
    <Owner>
        <ID>ironbucket</ID>
        <DisplayName>IronBucket</DisplayName>
    </Owner>
    <AccessControlList>
        <Grant>
            <Grantee xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
                     xsi:type="CanonicalUser">
                <ID>ironbucket</ID>
                <DisplayName>IronBucket</DisplayName>
            </Grantee>
            <Permission>FULL_CONTROL</Permission>
        </Grant>
    </AccessControlList>
</AccessControlPolicy>
```

---

## Object Operations

### Put Object

```http
PUT /{bucket}/{key}
```

**Headers:**
- `Content-Type`: Media type
- `Content-Length`: Object size
- `x-amz-meta-*`: Custom metadata
- `x-amz-storage-class`: Storage class
- `x-amz-server-side-encryption`: Encryption algorithm
- `Cache-Control`: Cache directive
- `Content-Disposition`: Display behavior
- `Content-Encoding`: Content encoding
- `Expires`: Expiration date

**Body:** Binary object data

**Response Headers:**
- `ETag`: MD5 hash of object
- `x-amz-version-id`: Version ID (if versioning enabled)

### Get Object

```http
GET /{bucket}/{key}
```

**Query Parameters:**
- `versionId`: Specific version to retrieve

**Request Headers:**
- `Range`: Byte range (e.g., `bytes=0-1023`)
- `If-Modified-Since`: Conditional request
- `If-None-Match`: Conditional request
- `If-Match`: Conditional request

**Response:** Binary object data with metadata headers

### Delete Object

```http
DELETE /{bucket}/{key}
```

**Query Parameters:**
- `versionId`: Specific version to delete

**Response Headers:**
- `x-amz-delete-marker`: `true` if delete marker created
- `x-amz-version-id`: Version ID of delete marker

### Head Object

```http
HEAD /{bucket}/{key}
```

**Response:** Metadata headers without body
- `Content-Type`
- `Content-Length`
- `Last-Modified`
- `ETag`
- `x-amz-meta-*`
- `x-amz-version-id`

### Copy Object

```http
PUT /{bucket}/{key}
```

**Headers:**
- `x-amz-copy-source`: Source bucket and key
- `x-amz-metadata-directive`: `COPY` or `REPLACE`

**Response:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<CopyObjectResult>
    <LastModified>2025-01-01T00:00:00.000Z</LastModified>
    <ETag>"d41d8cd98f00b204e9800998ecf8427e"</ETag>
</CopyObjectResult>
```

### Batch Delete

```http
POST /{bucket}?delete
```

**Body:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<Delete>
    <Quiet>false</Quiet>
    <Object>
        <Key>file1.txt</Key>
        <VersionId>v123456</VersionId>
    </Object>
    <Object>
        <Key>file2.txt</Key>
    </Object>
</Delete>
```

**Response:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<DeleteResult>
    <Deleted>
        <Key>file1.txt</Key>
        <VersionId>v123456</VersionId>
    </Deleted>
    <Error>
        <Key>file2.txt</Key>
        <Code>NoSuchKey</Code>
        <Message>The specified key does not exist</Message>
    </Error>
</DeleteResult>
```

---

## Multipart Upload

### Initiate Multipart Upload

```http
POST /{bucket}/{key}?uploads
```

**Headers:**
- Same as Put Object

**Response:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<InitiateMultipartUploadResult>
    <Bucket>my-bucket</Bucket>
    <Key>large-file.zip</Key>
    <UploadId>2~abcdef123456789</UploadId>
</InitiateMultipartUploadResult>
```

### Upload Part

```http
PUT /{bucket}/{key}?partNumber={partNumber}&uploadId={uploadId}
```

**Query Parameters:**
- `partNumber`: Part number (1-10000)
- `uploadId`: Upload ID from initiate

**Headers:**
- `Content-Length`: Part size

**Body:** Binary part data

**Response Headers:**
- `ETag`: Part's ETag

### List Parts

```http
GET /{bucket}/{key}?uploadId={uploadId}
```

**Query Parameters:**
- `max-parts`: Maximum parts to return
- `part-number-marker`: Start after this part

**Response:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<ListPartsResult>
    <Bucket>my-bucket</Bucket>
    <Key>large-file.zip</Key>
    <UploadId>2~abcdef123456789</UploadId>
    <PartNumberMarker>0</PartNumberMarker>
    <NextPartNumberMarker>2</NextPartNumberMarker>
    <MaxParts>1000</MaxParts>
    <IsTruncated>false</IsTruncated>

    <Part>
        <PartNumber>1</PartNumber>
        <LastModified>2025-01-01T00:00:00.000Z</LastModified>
        <ETag>"d41d8cd98f00b204e9800998ecf8427e"</ETag>
        <Size>5242880</Size>
    </Part>
</ListPartsResult>
```

### Complete Multipart Upload

```http
POST /{bucket}/{key}?uploadId={uploadId}
```

**Body:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<CompleteMultipartUpload>
    <Part>
        <PartNumber>1</PartNumber>
        <ETag>"d41d8cd98f00b204e9800998ecf8427e"</ETag>
    </Part>
    <Part>
        <PartNumber>2</PartNumber>
        <ETag>"e9800998ecf8427ed41d8cd98f00b204"</ETag>
    </Part>
</CompleteMultipartUpload>
```

**Response:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<CompleteMultipartUploadResult>
    <Location>http://localhost:9000/my-bucket/large-file.zip</Location>
    <Bucket>my-bucket</Bucket>
    <Key>large-file.zip</Key>
    <ETag>"3858f62230ac3c915f300c664312c11f-2"</ETag>
</CompleteMultipartUploadResult>
```

### Abort Multipart Upload

```http
DELETE /{bucket}/{key}?uploadId={uploadId}
```

**Response:** `204 No Content`

### List Multipart Uploads

```http
GET /{bucket}?uploads
```

**Query Parameters:**
- `prefix`: Filter by key prefix
- `delimiter`: Group by delimiter
- `max-uploads`: Maximum uploads to return
- `key-marker`: Continue after this key
- `upload-id-marker`: Continue after this upload ID

**Response:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<ListMultipartUploadsResult>
    <Bucket>my-bucket</Bucket>
    <KeyMarker></KeyMarker>
    <UploadIdMarker></UploadIdMarker>
    <NextKeyMarker>large-file.zip</NextKeyMarker>
    <NextUploadIdMarker>2~abcdef123456789</NextUploadIdMarker>
    <MaxUploads>1000</MaxUploads>
    <IsTruncated>false</IsTruncated>

    <Upload>
        <Key>large-file.zip</Key>
        <UploadId>2~abcdef123456789</UploadId>
        <Initiated>2025-01-01T00:00:00.000Z</Initiated>
    </Upload>
</ListMultipartUploadsResult>
```

---

## Advanced Operations

### Put Bucket Policy

```http
PUT /{bucket}?policy
```

**Body:**
```json
{
    "Version": "2012-10-17",
    "Statement": [{
        "Sid": "PublicReadGetObject",
        "Effect": "Allow",
        "Principal": "*",
        "Action": "s3:GetObject",
        "Resource": "arn:aws:s3:::my-bucket/*"
    }]
}
```

### Get Bucket Policy

```http
GET /{bucket}?policy
```

**Response:** JSON policy document

### Delete Bucket Policy

```http
DELETE /{bucket}?policy
```

### Put Bucket Encryption

```http
PUT /{bucket}?encryption
```

**Body:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<ServerSideEncryptionConfiguration>
    <Rule>
        <ApplyServerSideEncryptionByDefault>
            <SSEAlgorithm>AES256</SSEAlgorithm>
        </ApplyServerSideEncryptionByDefault>
    </Rule>
</ServerSideEncryptionConfiguration>
```

### Get Bucket Encryption

```http
GET /{bucket}?encryption
```

### Delete Bucket Encryption

```http
DELETE /{bucket}?encryption
```

### Put Bucket CORS

```http
PUT /{bucket}?cors
```

**Body:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<CORSConfiguration>
    <CORSRule>
        <AllowedOrigin>*</AllowedOrigin>
        <AllowedMethod>GET</AllowedMethod>
        <AllowedMethod>PUT</AllowedMethod>
        <AllowedHeader>*</AllowedHeader>
        <MaxAgeSeconds>3000</MaxAgeSeconds>
        <ExposeHeader>ETag</ExposeHeader>
    </CORSRule>
</CORSConfiguration>
```

### Get Bucket CORS

```http
GET /{bucket}?cors
```

### Delete Bucket CORS

```http
DELETE /{bucket}?cors
```

### Put Bucket Lifecycle

```http
PUT /{bucket}?lifecycle
```

**Body:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<LifecycleConfiguration>
    <Rule>
        <ID>delete-old-logs</ID>
        <Status>Enabled</Status>
        <Filter>
            <Prefix>logs/</Prefix>
        </Filter>
        <Expiration>
            <Days>30</Days>
        </Expiration>
    </Rule>
</LifecycleConfiguration>
```

### Get Bucket Lifecycle

```http
GET /{bucket}?lifecycle
```

### Delete Bucket Lifecycle

```http
DELETE /{bucket}?lifecycle
```

### List Object Versions

```http
GET /{bucket}?versions
```

**Query Parameters:**
- `prefix`: Filter by prefix
- `delimiter`: Group by delimiter
- `max-keys`: Maximum results
- `key-marker`: Continue after this key
- `version-id-marker`: Continue after this version

**Response:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<ListVersionsResult>
    <Name>my-bucket</Name>
    <Prefix></Prefix>
    <MaxKeys>1000</MaxKeys>
    <IsTruncated>false</IsTruncated>

    <Version>
        <Key>file.txt</Key>
        <VersionId>v123456</VersionId>
        <IsLatest>true</IsLatest>
        <LastModified>2025-01-01T00:00:00.000Z</LastModified>
        <ETag>"d41d8cd98f00b204e9800998ecf8427e"</ETag>
        <Size>1024</Size>
        <StorageClass>STANDARD</StorageClass>
    </Version>

    <DeleteMarker>
        <Key>deleted.txt</Key>
        <VersionId>v789012</VersionId>
        <IsLatest>true</IsLatest>
        <LastModified>2025-01-02T00:00:00.000Z</LastModified>
    </DeleteMarker>
</ListVersionsResult>
```

---

## Request Examples

### cURL Examples

```bash
# List buckets
curl -X GET http://localhost:9000/ \
  -H "Authorization: AWS4-HMAC-SHA256 ..."

# Create bucket
curl -X PUT http://localhost:9000/my-bucket \
  -H "Authorization: AWS4-HMAC-SHA256 ..."

# Upload object
curl -X PUT http://localhost:9000/my-bucket/file.txt \
  -H "Authorization: AWS4-HMAC-SHA256 ..." \
  -H "Content-Type: text/plain" \
  --data-binary @file.txt

# Download object
curl -X GET http://localhost:9000/my-bucket/file.txt \
  -H "Authorization: AWS4-HMAC-SHA256 ..." \
  -o downloaded.txt
```

---

## Rate Limiting

IronBucket does not impose rate limits by default. For production deployments, consider implementing rate limiting at the proxy level.

---

## Monitoring Endpoints

### Health Check

```http
GET /health
```

**Response:**
```json
{
    "status": "healthy",
    "version": "1.0.0",
    "uptime": 3600
}
```

### Metrics

```http
GET /metrics
```

**Response:** Prometheus-compatible metrics

---

*For usage examples in different programming languages, see the [Usage Guides](README.md#usage-examples).*