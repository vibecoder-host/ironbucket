# Node.js Usage Guide

This guide demonstrates how to use IronBucket with Node.js applications using the AWS SDK for JavaScript v3.

## Table of Contents

- [Installation](#installation)
- [Configuration](#configuration)
- [Basic Operations](#basic-operations)
- [Advanced Operations](#advanced-operations)
- [Multipart Upload](#multipart-upload)
- [Error Handling](#error-handling)
- [Best Practices](#best-practices)
- [Complete Examples](#complete-examples)

## Installation

Install the AWS SDK for JavaScript v3:

```bash
npm install @aws-sdk/client-s3
npm install @aws-sdk/lib-storage  # For multipart uploads
npm install @aws-sdk/s3-request-presigner  # For presigned URLs
```

## Configuration

### Basic Client Setup

```javascript
const { S3Client } = require('@aws-sdk/client-s3');

// Using environment variables (recommended)
const s3Client = new S3Client({
    endpoint: process.env.IRONBUCKET_ENDPOINT || 'http://localhost:9000',
    region: process.env.IRONBUCKET_REGION || 'us-east-1',
    credentials: {
        accessKeyId: process.env.IRONBUCKET_ACCESS_KEY,
        secretAccessKey: process.env.IRONBUCKET_SECRET_KEY
    },
    forcePathStyle: true  // Required for S3-compatible services
});
```

### Environment Variables Configuration

```javascript
// .env file (create this in your project root)
IRONBUCKET_ENDPOINT=http://localhost:9000
IRONBUCKET_ACCESS_KEY=your-access-key
IRONBUCKET_SECRET_KEY=your-secret-key
IRONBUCKET_REGION=us-east-1

// app.js
require('dotenv').config();

const s3Client = new S3Client({
    endpoint: process.env.IRONBUCKET_ENDPOINT,
    region: process.env.IRONBUCKET_REGION || 'us-east-1',
    credentials: {
        accessKeyId: process.env.IRONBUCKET_ACCESS_KEY,
        secretAccessKey: process.env.IRONBUCKET_SECRET_KEY
    },
    forcePathStyle: true
});

// Validate required environment variables
if (!process.env.IRONBUCKET_ACCESS_KEY || !process.env.IRONBUCKET_SECRET_KEY) {
    throw new Error('Missing required environment variables: IRONBUCKET_ACCESS_KEY and IRONBUCKET_SECRET_KEY');
}
```

## Basic Operations

### List Buckets

```javascript
const { ListBucketsCommand } = require('@aws-sdk/client-s3');

async function listBuckets() {
    try {
        const command = new ListBucketsCommand({});
        const response = await s3Client.send(command);

        console.log('Buckets:');
        response.Buckets.forEach(bucket => {
            console.log(`  - ${bucket.Name} (Created: ${bucket.CreationDate})`);
        });

        return response.Buckets;
    } catch (error) {
        console.error('Error listing buckets:', error);
        throw error;
    }
}
```

### Create Bucket

```javascript
const { CreateBucketCommand } = require('@aws-sdk/client-s3');

async function createBucket(bucketName) {
    try {
        const command = new CreateBucketCommand({
            Bucket: bucketName
        });

        const response = await s3Client.send(command);
        console.log(`Bucket "${bucketName}" created successfully`);
        return response;
    } catch (error) {
        if (error.name === 'BucketAlreadyExists') {
            console.log('Bucket already exists');
        } else {
            console.error('Error creating bucket:', error);
            throw error;
        }
    }
}
```

### Upload Object

```javascript
const { PutObjectCommand } = require('@aws-sdk/client-s3');
const fs = require('fs');

async function uploadFile(bucketName, key, filePath) {
    try {
        const fileStream = fs.createReadStream(filePath);
        const uploadParams = {
            Bucket: bucketName,
            Key: key,
            Body: fileStream,
            ContentType: 'application/octet-stream',  // Adjust based on file type
            Metadata: {
                'uploaded-by': 'nodejs-app',
                'upload-date': new Date().toISOString()
            }
        };

        const command = new PutObjectCommand(uploadParams);
        const response = await s3Client.send(command);

        console.log(`File uploaded successfully. ETag: ${response.ETag}`);
        return response;
    } catch (error) {
        console.error('Error uploading file:', error);
        throw error;
    }
}

// Upload with content directly
async function uploadContent(bucketName, key, content) {
    try {
        const command = new PutObjectCommand({
            Bucket: bucketName,
            Key: key,
            Body: content,
            ContentType: 'text/plain'
        });

        const response = await s3Client.send(command);
        console.log(`Content uploaded successfully. ETag: ${response.ETag}`);
        return response;
    } catch (error) {
        console.error('Error uploading content:', error);
        throw error;
    }
}
```

### Download Object

```javascript
const { GetObjectCommand } = require('@aws-sdk/client-s3');
const fs = require('fs');
const { pipeline } = require('stream/promises');

async function downloadFile(bucketName, key, downloadPath) {
    try {
        const command = new GetObjectCommand({
            Bucket: bucketName,
            Key: key
        });

        const response = await s3Client.send(command);

        // Save to file
        const writeStream = fs.createWriteStream(downloadPath);
        await pipeline(response.Body, writeStream);

        console.log(`File downloaded to ${downloadPath}`);
        return response;
    } catch (error) {
        console.error('Error downloading file:', error);
        throw error;
    }
}

// Download to memory
async function downloadToMemory(bucketName, key) {
    try {
        const command = new GetObjectCommand({
            Bucket: bucketName,
            Key: key
        });

        const response = await s3Client.send(command);
        const chunks = [];

        for await (const chunk of response.Body) {
            chunks.push(chunk);
        }

        const content = Buffer.concat(chunks).toString('utf-8');
        return content;
    } catch (error) {
        console.error('Error downloading object:', error);
        throw error;
    }
}
```

### List Objects

```javascript
const { ListObjectsV2Command } = require('@aws-sdk/client-s3');

async function listObjects(bucketName, prefix = '', maxKeys = 1000) {
    try {
        const command = new ListObjectsV2Command({
            Bucket: bucketName,
            Prefix: prefix,
            MaxKeys: maxKeys
        });

        const response = await s3Client.send(command);

        console.log(`Objects in ${bucketName}:`);
        response.Contents?.forEach(object => {
            console.log(`  - ${object.Key} (Size: ${object.Size}, Modified: ${object.LastModified})`);
        });

        return response.Contents || [];
    } catch (error) {
        console.error('Error listing objects:', error);
        throw error;
    }
}

// List with pagination
async function* listAllObjects(bucketName, prefix = '') {
    let continuationToken = undefined;

    do {
        const command = new ListObjectsV2Command({
            Bucket: bucketName,
            Prefix: prefix,
            MaxKeys: 1000,
            ContinuationToken: continuationToken
        });

        const response = await s3Client.send(command);

        if (response.Contents) {
            yield* response.Contents;
        }

        continuationToken = response.NextContinuationToken;
    } while (continuationToken);
}

// Usage
async function getAllObjects(bucketName) {
    const objects = [];
    for await (const object of listAllObjects(bucketName)) {
        objects.push(object);
    }
    console.log(`Total objects: ${objects.length}`);
    return objects;
}
```

### Delete Object

```javascript
const { DeleteObjectCommand } = require('@aws-sdk/client-s3');

async function deleteObject(bucketName, key) {
    try {
        const command = new DeleteObjectCommand({
            Bucket: bucketName,
            Key: key
        });

        const response = await s3Client.send(command);
        console.log(`Object "${key}" deleted successfully`);
        return response;
    } catch (error) {
        console.error('Error deleting object:', error);
        throw error;
    }
}
```

### Delete Multiple Objects

```javascript
const { DeleteObjectsCommand } = require('@aws-sdk/client-s3');

async function deleteMultipleObjects(bucketName, keys) {
    try {
        const objects = keys.map(key => ({ Key: key }));

        const command = new DeleteObjectsCommand({
            Bucket: bucketName,
            Delete: {
                Objects: objects,
                Quiet: false  // Set to true to suppress success messages
            }
        });

        const response = await s3Client.send(command);

        if (response.Deleted) {
            console.log('Deleted objects:');
            response.Deleted.forEach(obj => {
                console.log(`  - ${obj.Key}`);
            });
        }

        if (response.Errors) {
            console.log('Errors:');
            response.Errors.forEach(err => {
                console.log(`  - ${err.Key}: ${err.Message}`);
            });
        }

        return response;
    } catch (error) {
        console.error('Error deleting objects:', error);
        throw error;
    }
}
```

## Advanced Operations

### Get Object Metadata

```javascript
const { HeadObjectCommand } = require('@aws-sdk/client-s3');

async function getObjectMetadata(bucketName, key) {
    try {
        const command = new HeadObjectCommand({
            Bucket: bucketName,
            Key: key
        });

        const response = await s3Client.send(command);

        console.log('Object Metadata:');
        console.log(`  ContentType: ${response.ContentType}`);
        console.log(`  ContentLength: ${response.ContentLength}`);
        console.log(`  LastModified: ${response.LastModified}`);
        console.log(`  ETag: ${response.ETag}`);
        console.log(`  Metadata:`, response.Metadata);

        return response;
    } catch (error) {
        if (error.name === 'NotFound') {
            console.log('Object not found');
        } else {
            console.error('Error getting metadata:', error);
            throw error;
        }
    }
}
```

### Copy Object

```javascript
const { CopyObjectCommand } = require('@aws-sdk/client-s3');

async function copyObject(sourceBucket, sourceKey, destBucket, destKey) {
    try {
        const command = new CopyObjectCommand({
            CopySource: `${sourceBucket}/${sourceKey}`,
            Bucket: destBucket,
            Key: destKey,
            MetadataDirective: 'COPY'  // or 'REPLACE' to set new metadata
        });

        const response = await s3Client.send(command);
        console.log(`Object copied successfully. ETag: ${response.CopyObjectResult.ETag}`);
        return response;
    } catch (error) {
        console.error('Error copying object:', error);
        throw error;
    }
}
```

### Generate Presigned URL

```javascript
const { GetObjectCommand, PutObjectCommand } = require('@aws-sdk/client-s3');
const { getSignedUrl } = require('@aws-sdk/s3-request-presigner');

// Presigned URL for download
async function getPresignedDownloadUrl(bucketName, key, expiresIn = 3600) {
    try {
        const command = new GetObjectCommand({
            Bucket: bucketName,
            Key: key
        });

        const url = await getSignedUrl(s3Client, command, { expiresIn });
        console.log(`Presigned download URL: ${url}`);
        return url;
    } catch (error) {
        console.error('Error generating presigned URL:', error);
        throw error;
    }
}

// Presigned URL for upload
async function getPresignedUploadUrl(bucketName, key, expiresIn = 3600) {
    try {
        const command = new PutObjectCommand({
            Bucket: bucketName,
            Key: key
        });

        const url = await getSignedUrl(s3Client, command, { expiresIn });
        console.log(`Presigned upload URL: ${url}`);
        return url;
    } catch (error) {
        console.error('Error generating presigned URL:', error);
        throw error;
    }
}
```

## Multipart Upload

For large files (> 100MB), use multipart upload:

```javascript
const { Upload } = require('@aws-sdk/lib-storage');
const fs = require('fs');

async function uploadLargeFile(bucketName, key, filePath) {
    try {
        const fileStream = fs.createReadStream(filePath);
        const uploadParams = {
            Bucket: bucketName,
            Key: key,
            Body: fileStream
        };

        const upload = new Upload({
            client: s3Client,
            params: uploadParams,
            queueSize: 4,  // Concurrent parts
            partSize: 5 * 1024 * 1024,  // 5MB per part
            leavePartsOnError: false
        });

        // Track upload progress
        upload.on('httpUploadProgress', (progress) => {
            console.log(`Uploaded ${progress.loaded} of ${progress.total} bytes`);
        });

        const response = await upload.done();
        console.log(`Large file uploaded successfully. Location: ${response.Location}`);
        return response;
    } catch (error) {
        console.error('Error uploading large file:', error);
        throw error;
    }
}
```

### Manual Multipart Upload

```javascript
const {
    CreateMultipartUploadCommand,
    UploadPartCommand,
    CompleteMultipartUploadCommand,
    AbortMultipartUploadCommand
} = require('@aws-sdk/client-s3');
const fs = require('fs');

async function manualMultipartUpload(bucketName, key, filePath) {
    const fileStats = fs.statSync(filePath);
    const fileSize = fileStats.size;
    const partSize = 5 * 1024 * 1024; // 5MB
    const numParts = Math.ceil(fileSize / partSize);

    let uploadId;
    const parts = [];

    try {
        // Initiate multipart upload
        const initCommand = new CreateMultipartUploadCommand({
            Bucket: bucketName,
            Key: key
        });
        const initResponse = await s3Client.send(initCommand);
        uploadId = initResponse.UploadId;
        console.log(`Multipart upload initiated. UploadId: ${uploadId}`);

        // Upload parts
        const fileStream = fs.createReadStream(filePath);

        for (let partNumber = 1; partNumber <= numParts; partNumber++) {
            const start = (partNumber - 1) * partSize;
            const end = Math.min(start + partSize, fileSize);

            const partStream = fs.createReadStream(filePath, {
                start: start,
                end: end - 1
            });

            const uploadPartCommand = new UploadPartCommand({
                Bucket: bucketName,
                Key: key,
                PartNumber: partNumber,
                UploadId: uploadId,
                Body: partStream
            });

            const partResponse = await s3Client.send(uploadPartCommand);

            parts.push({
                ETag: partResponse.ETag,
                PartNumber: partNumber
            });

            console.log(`Part ${partNumber}/${numParts} uploaded`);
        }

        // Complete multipart upload
        const completeCommand = new CompleteMultipartUploadCommand({
            Bucket: bucketName,
            Key: key,
            UploadId: uploadId,
            MultipartUpload: { Parts: parts }
        });

        const completeResponse = await s3Client.send(completeCommand);
        console.log(`Multipart upload completed. Location: ${completeResponse.Location}`);
        return completeResponse;

    } catch (error) {
        // Abort multipart upload on error
        if (uploadId) {
            const abortCommand = new AbortMultipartUploadCommand({
                Bucket: bucketName,
                Key: key,
                UploadId: uploadId
            });
            await s3Client.send(abortCommand);
            console.log('Multipart upload aborted');
        }
        throw error;
    }
}
```

## Error Handling

### Comprehensive Error Handling

```javascript
async function safeS3Operation(operation) {
    try {
        return await operation();
    } catch (error) {
        // Handle specific S3 errors
        switch (error.name) {
            case 'NoSuchBucket':
                console.error('Bucket does not exist');
                break;
            case 'NoSuchKey':
                console.error('Object does not exist');
                break;
            case 'BucketAlreadyExists':
            case 'BucketAlreadyOwnedByYou':
                console.error('Bucket already exists');
                break;
            case 'AccessDenied':
                console.error('Access denied. Check your credentials');
                break;
            case 'InvalidAccessKeyId':
                console.error('Invalid access key ID');
                break;
            case 'SignatureDoesNotMatch':
                console.error('Invalid secret access key');
                break;
            case 'RequestTimeout':
                console.error('Request timed out');
                break;
            default:
                console.error(`Unexpected error: ${error.name}`, error.message);
        }
        throw error;
    }
}

// Usage
await safeS3Operation(async () => {
    return await uploadFile('my-bucket', 'file.txt', './file.txt');
});
```

### Retry Logic

```javascript
async function withRetry(operation, maxRetries = 3, delay = 1000) {
    for (let attempt = 1; attempt <= maxRetries; attempt++) {
        try {
            return await operation();
        } catch (error) {
            if (attempt === maxRetries) {
                throw error;
            }

            console.log(`Attempt ${attempt} failed, retrying in ${delay}ms...`);
            await new Promise(resolve => setTimeout(resolve, delay));
            delay *= 2; // Exponential backoff
        }
    }
}

// Usage
const result = await withRetry(async () => {
    return await uploadFile('my-bucket', 'file.txt', './file.txt');
});
```

## Best Practices

### 1. Connection Pooling

```javascript
const { NodeHttpHandler } = require('@aws-sdk/node-http-handler');
const https = require('https');

const s3Client = new S3Client({
    endpoint: process.env.IRONBUCKET_ENDPOINT || 'http://localhost:9000',
    region: process.env.IRONBUCKET_REGION || 'us-east-1',
    credentials: {
        accessKeyId: process.env.IRONBUCKET_ACCESS_KEY,
        secretAccessKey: process.env.IRONBUCKET_SECRET_KEY
    },
    forcePathStyle: true,
    requestHandler: new NodeHttpHandler({
        httpsAgent: new https.Agent({
            keepAlive: true,
            maxSockets: 50
        })
    })
});
```

### 2. Stream Processing

```javascript
const stream = require('stream');
const { Transform } = stream;

// Process large files with streams
async function processLargeFile(bucketName, key) {
    const command = new GetObjectCommand({
        Bucket: bucketName,
        Key: key
    });

    const response = await s3Client.send(command);

    // Create transform stream for processing
    const processStream = new Transform({
        transform(chunk, encoding, callback) {
            // Process chunk (e.g., convert to uppercase)
            const processed = chunk.toString().toUpperCase();
            callback(null, Buffer.from(processed));
        }
    });

    // Pipe through processing
    response.Body.pipe(processStream)
        .pipe(fs.createWriteStream('processed-output.txt'));
}
```

### 3. Batch Operations

```javascript
async function batchUpload(bucketName, files) {
    const uploadPromises = files.map(file =>
        uploadFile(bucketName, file.key, file.path)
            .catch(error => ({
                key: file.key,
                error: error.message
            }))
    );

    const results = await Promise.allSettled(uploadPromises);

    const successful = results.filter(r => r.status === 'fulfilled');
    const failed = results.filter(r => r.status === 'rejected');

    console.log(`Uploaded: ${successful.length}, Failed: ${failed.length}`);
    return { successful, failed };
}
```

### 4. Memory Management

```javascript
// Stream large downloads directly to disk
async function downloadLargeFile(bucketName, key, outputPath) {
    const command = new GetObjectCommand({
        Bucket: bucketName,
        Key: key
    });

    const response = await s3Client.send(command);
    const writeStream = fs.createWriteStream(outputPath);

    return new Promise((resolve, reject) => {
        response.Body
            .pipe(writeStream)
            .on('finish', resolve)
            .on('error', reject);
    });
}
```

## Complete Examples

### Example 1: File Sync Application

```javascript
const path = require('path');
const crypto = require('crypto');

class S3FileSync {
    constructor(s3Client, bucketName) {
        this.s3Client = s3Client;
        this.bucketName = bucketName;
    }

    async syncDirectory(localDir, s3Prefix = '') {
        const localFiles = await this.getLocalFiles(localDir);
        const s3Objects = await this.getS3Objects(s3Prefix);

        const toUpload = [];
        const toDelete = [];

        // Find files to upload
        for (const localFile of localFiles) {
            const s3Key = path.join(s3Prefix, localFile.relativePath);
            const s3Object = s3Objects.find(obj => obj.Key === s3Key);

            if (!s3Object || localFile.hash !== s3Object.ETag.replace(/"/g, '')) {
                toUpload.push({
                    path: localFile.path,
                    key: s3Key
                });
            }
        }

        // Find objects to delete
        for (const s3Object of s3Objects) {
            const localFile = localFiles.find(
                f => path.join(s3Prefix, f.relativePath) === s3Object.Key
            );
            if (!localFile) {
                toDelete.push(s3Object.Key);
            }
        }

        // Perform sync
        console.log(`Uploading ${toUpload.length} files...`);
        for (const file of toUpload) {
            await this.uploadFile(file.path, file.key);
        }

        console.log(`Deleting ${toDelete.length} objects...`);
        if (toDelete.length > 0) {
            await this.deleteObjects(toDelete);
        }

        console.log('Sync complete!');
    }

    async getLocalFiles(dir, baseDir = dir) {
        const files = [];
        const entries = fs.readdirSync(dir, { withFileTypes: true });

        for (const entry of entries) {
            const fullPath = path.join(dir, entry.name);

            if (entry.isDirectory()) {
                files.push(...await this.getLocalFiles(fullPath, baseDir));
            } else {
                const hash = await this.getFileHash(fullPath);
                files.push({
                    path: fullPath,
                    relativePath: path.relative(baseDir, fullPath),
                    hash: hash
                });
            }
        }

        return files;
    }

    async getFileHash(filePath) {
        return new Promise((resolve, reject) => {
            const hash = crypto.createHash('md5');
            const stream = fs.createReadStream(filePath);

            stream.on('data', data => hash.update(data));
            stream.on('end', () => resolve(hash.digest('hex')));
            stream.on('error', reject);
        });
    }

    async getS3Objects(prefix) {
        const objects = [];

        for await (const object of listAllObjects(this.bucketName, prefix)) {
            objects.push(object);
        }

        return objects;
    }

    async uploadFile(localPath, s3Key) {
        const fileStream = fs.createReadStream(localPath);
        const command = new PutObjectCommand({
            Bucket: this.bucketName,
            Key: s3Key,
            Body: fileStream
        });

        await this.s3Client.send(command);
        console.log(`Uploaded: ${s3Key}`);
    }

    async deleteObjects(keys) {
        const command = new DeleteObjectsCommand({
            Bucket: this.bucketName,
            Delete: {
                Objects: keys.map(key => ({ Key: key })),
                Quiet: true
            }
        });

        await this.s3Client.send(command);
    }
}

// Usage
const sync = new S3FileSync(s3Client, 'my-bucket');
await sync.syncDirectory('./local-folder', 'remote-folder/');
```

### Example 2: Image Processing Pipeline

```javascript
const sharp = require('sharp'); // npm install sharp

class S3ImageProcessor {
    constructor(s3Client) {
        this.s3Client = s3Client;
    }

    async processImage(sourceBucket, sourceKey, destBucket) {
        // Download original image
        const getCommand = new GetObjectCommand({
            Bucket: sourceBucket,
            Key: sourceKey
        });

        const response = await this.s3Client.send(getCommand);
        const chunks = [];

        for await (const chunk of response.Body) {
            chunks.push(chunk);
        }

        const imageBuffer = Buffer.concat(chunks);

        // Generate multiple sizes
        const sizes = [
            { name: 'thumbnail', width: 150, height: 150 },
            { name: 'small', width: 320, height: 240 },
            { name: 'medium', width: 640, height: 480 },
            { name: 'large', width: 1024, height: 768 }
        ];

        for (const size of sizes) {
            const resizedBuffer = await sharp(imageBuffer)
                .resize(size.width, size.height, {
                    fit: 'inside',
                    withoutEnlargement: true
                })
                .jpeg({ quality: 80 })
                .toBuffer();

            const newKey = sourceKey.replace(
                /(\.[^.]+)$/,
                `-${size.name}$1`
            );

            const putCommand = new PutObjectCommand({
                Bucket: destBucket,
                Key: newKey,
                Body: resizedBuffer,
                ContentType: 'image/jpeg',
                Metadata: {
                    'original-key': sourceKey,
                    'size-preset': size.name,
                    'processed-at': new Date().toISOString()
                }
            });

            await this.s3Client.send(putCommand);
            console.log(`Created ${size.name}: ${newKey}`);
        }
    }
}

// Usage
const processor = new S3ImageProcessor(s3Client);
await processor.processImage('uploads', 'photo.jpg', 'processed');
```

## Testing

### Unit Testing with Jest

```javascript
// s3-service.test.js
const { S3Client } = require('@aws-sdk/client-s3');
const { mockClient } = require('aws-sdk-client-mock');

const s3Mock = mockClient(S3Client);

beforeEach(() => {
    s3Mock.reset();
});

test('should upload file successfully', async () => {
    const putObjectResponse = {
        ETag: '"test-etag"',
        VersionId: 'test-version'
    };

    s3Mock.on(PutObjectCommand).resolves(putObjectResponse);

    const result = await uploadFile('test-bucket', 'test-key', './test.txt');

    expect(result.ETag).toBe('"test-etag"');
    expect(s3Mock.calls()).toHaveLength(1);
});
```

### Integration Testing

```javascript
// integration-test.js
const TEST_BUCKET = 'test-bucket-' + Date.now();

describe('IronBucket Integration Tests', () => {
    beforeAll(async () => {
        await createBucket(TEST_BUCKET);
    });

    afterAll(async () => {
        // Clean up
        const objects = await listObjects(TEST_BUCKET);
        if (objects.length > 0) {
            await deleteMultipleObjects(
                TEST_BUCKET,
                objects.map(obj => obj.Key)
            );
        }
        await deleteBucket(TEST_BUCKET);
    });

    test('complete upload and download cycle', async () => {
        const testKey = 'test-file.txt';
        const testContent = 'Hello, IronBucket!';

        // Upload
        await uploadContent(TEST_BUCKET, testKey, testContent);

        // Download
        const downloaded = await downloadToMemory(TEST_BUCKET, testKey);

        expect(downloaded).toBe(testContent);
    });
});
```

## Performance Tips

1. **Use streaming for large files** - Avoid loading entire files into memory
2. **Enable connection keep-alive** - Reuse HTTP connections
3. **Batch operations when possible** - Reduce API calls
4. **Use multipart upload for files > 100MB** - Improve upload reliability
5. **Implement exponential backoff** - Handle rate limiting gracefully
6. **Cache frequently accessed objects** - Reduce S3 requests
7. **Use presigned URLs for direct uploads** - Offload work from your server

## Troubleshooting

### Common Issues

1. **ECONNREFUSED Error**
   - Check if IronBucket is running
   - Verify the endpoint URL and port
   - Check firewall settings

2. **SignatureDoesNotMatch Error**
   - Verify credentials are correct
   - Check system time is synchronized
   - Ensure region is set (even if arbitrary)

3. **Slow Upload/Download**
   - Use multipart upload for large files
   - Check network connectivity
   - Increase concurrent connections

4. **Memory Issues**
   - Use streams instead of buffers
   - Process files in chunks
   - Implement pagination for large listings

## Additional Resources

- [AWS SDK for JavaScript v3 Documentation](https://docs.aws.amazon.com/AWSJavaScriptSDK/v3/latest/)
- [S3 API Reference](https://docs.aws.amazon.com/AmazonS3/latest/API/)
- [IronBucket API Documentation](./API.md)
- [IronBucket Configuration Guide](../README.md#configuration)