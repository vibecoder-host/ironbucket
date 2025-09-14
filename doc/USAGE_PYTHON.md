# Python Usage Guide

This guide demonstrates how to use IronBucket with Python applications using boto3.

## Table of Contents

- [Installation](#installation)
- [Configuration](#configuration)
- [Basic Operations](#basic-operations)
- [Advanced Operations](#advanced-operations)
- [Multipart Upload](#multipart-upload)
- [Error Handling](#error-handling)
- [Async Operations with aioboto3](#async-operations-with-aioboto3)
- [Best Practices](#best-practices)
- [Complete Examples](#complete-examples)

## Installation

Install boto3 and related packages:

```bash
# Basic installation
pip install boto3

# For async operations
pip install aioboto3

# For better performance
pip install boto3[crt]

# Additional utilities
pip install python-dotenv  # For environment variables
pip install tqdm  # For progress bars
```

## Configuration

### Basic Client Setup

```python
import os
import boto3
from botocore.config import Config

# Using environment variables (recommended)
s3_client = boto3.client(
    's3',
    endpoint_url=os.environ.get('IRONBUCKET_ENDPOINT', 'http://localhost:9000'),
    aws_access_key_id=os.environ.get('IRONBUCKET_ACCESS_KEY'),
    aws_secret_access_key=os.environ.get('IRONBUCKET_SECRET_KEY'),
    region_name=os.environ.get('IRONBUCKET_REGION', 'us-east-1'),
    config=Config(
        signature_version='s3v4',
        retries={'max_attempts': 3, 'mode': 'standard'}
    )
)

# Create S3 resource (higher-level interface)
s3_resource = boto3.resource(
    's3',
    endpoint_url=os.environ.get('IRONBUCKET_ENDPOINT', 'http://localhost:9000'),
    aws_access_key_id=os.environ.get('IRONBUCKET_ACCESS_KEY'),
    aws_secret_access_key=os.environ.get('IRONBUCKET_SECRET_KEY'),
    region_name=os.environ.get('IRONBUCKET_REGION', 'us-east-1')
)

# Validate required environment variables
if not os.environ.get('IRONBUCKET_ACCESS_KEY') or not os.environ.get('IRONBUCKET_SECRET_KEY'):
    raise ValueError('Missing required environment variables: IRONBUCKET_ACCESS_KEY and IRONBUCKET_SECRET_KEY')
```

### Environment Variables Configuration

```python
# .env file (create this in your project root)
IRONBUCKET_ENDPOINT=http://localhost:9000
IRONBUCKET_ACCESS_KEY=your-access-key
IRONBUCKET_SECRET_KEY=your-secret-key
IRONBUCKET_REGION=us-east-1

# app.py
import os
from dotenv import load_dotenv
import boto3

load_dotenv()

s3_client = boto3.client(
    's3',
    endpoint_url=os.getenv('IRONBUCKET_ENDPOINT'),
    aws_access_key_id=os.getenv('IRONBUCKET_ACCESS_KEY'),
    aws_secret_access_key=os.getenv('IRONBUCKET_SECRET_KEY'),
    region_name=os.getenv('IRONBUCKET_REGION', 'us-east-1')
)

# Validate credentials are set
if not all([os.getenv('IRONBUCKET_ACCESS_KEY'), os.getenv('IRONBUCKET_SECRET_KEY')]):
    raise ValueError('Missing required IronBucket credentials in environment variables')
```

### Configuration Class

```python
class S3Config:
    """S3 configuration wrapper"""

    def __init__(self, endpoint_url=None, access_key=None, secret_key=None):
        self.endpoint_url = endpoint_url or os.getenv('IRONBUCKET_ENDPOINT', 'http://localhost:9000')
        self.access_key = access_key or os.getenv('IRONBUCKET_ACCESS_KEY')
        self.secret_key = secret_key or os.getenv('IRONBUCKET_SECRET_KEY')
        self.region = os.getenv('IRONBUCKET_REGION', 'us-east-1')

    def create_client(self):
        return boto3.client(
            's3',
            endpoint_url=self.endpoint_url,
            aws_access_key_id=self.access_key,
            aws_secret_access_key=self.secret_key,
            region_name=self.region
        )

    def create_resource(self):
        return boto3.resource(
            's3',
            endpoint_url=self.endpoint_url,
            aws_access_key_id=self.access_key,
            aws_secret_access_key=self.secret_key,
            region_name=self.region
        )
```

## Basic Operations

### List Buckets

```python
def list_buckets():
    """List all buckets"""
    try:
        response = s3_client.list_buckets()

        print("Buckets:")
        for bucket in response['Buckets']:
            print(f"  - {bucket['Name']} (Created: {bucket['CreationDate']})")

        return response['Buckets']
    except Exception as e:
        print(f"Error listing buckets: {e}")
        raise
```

### Create Bucket

```python
def create_bucket(bucket_name):
    """Create a new bucket"""
    try:
        response = s3_client.create_bucket(Bucket=bucket_name)
        print(f"Bucket '{bucket_name}' created successfully")
        return response
    except s3_client.exceptions.BucketAlreadyExists:
        print(f"Bucket '{bucket_name}' already exists")
    except s3_client.exceptions.BucketAlreadyOwnedByYou:
        print(f"Bucket '{bucket_name}' already owned by you")
    except Exception as e:
        print(f"Error creating bucket: {e}")
        raise
```

### Upload Object

```python
def upload_file(bucket_name, key, file_path, metadata=None):
    """Upload a file to S3"""
    try:
        extra_args = {}

        # Add metadata if provided
        if metadata:
            extra_args['Metadata'] = metadata

        # Detect content type
        import mimetypes
        content_type, _ = mimetypes.guess_type(file_path)
        if content_type:
            extra_args['ContentType'] = content_type

        # Upload with progress callback
        def progress_callback(bytes_transferred):
            print(f"Uploaded {bytes_transferred} bytes")

        s3_client.upload_file(
            file_path,
            bucket_name,
            key,
            ExtraArgs=extra_args,
            Callback=progress_callback
        )

        print(f"File uploaded successfully to s3://{bucket_name}/{key}")
        return True
    except Exception as e:
        print(f"Error uploading file: {e}")
        raise

# Upload with content directly
def upload_content(bucket_name, key, content, content_type='text/plain'):
    """Upload content directly to S3"""
    try:
        response = s3_client.put_object(
            Bucket=bucket_name,
            Key=key,
            Body=content,
            ContentType=content_type,
            Metadata={
                'uploaded-by': 'python-app',
                'upload-date': datetime.now().isoformat()
            }
        )

        print(f"Content uploaded successfully. ETag: {response['ETag']}")
        return response
    except Exception as e:
        print(f"Error uploading content: {e}")
        raise
```

### Download Object

```python
def download_file(bucket_name, key, download_path):
    """Download a file from S3"""
    try:
        # Download with progress tracking
        def progress_callback(bytes_transferred):
            print(f"Downloaded {bytes_transferred} bytes")

        s3_client.download_file(
            bucket_name,
            key,
            download_path,
            Callback=progress_callback
        )

        print(f"File downloaded to {download_path}")
        return True
    except s3_client.exceptions.NoSuchKey:
        print(f"Object {key} not found in bucket {bucket_name}")
        return False
    except Exception as e:
        print(f"Error downloading file: {e}")
        raise

# Download to memory
def download_to_memory(bucket_name, key):
    """Download object content to memory"""
    try:
        response = s3_client.get_object(Bucket=bucket_name, Key=key)
        content = response['Body'].read()

        # For text files
        text_content = content.decode('utf-8')
        return text_content
    except Exception as e:
        print(f"Error downloading to memory: {e}")
        raise
```

### List Objects

```python
def list_objects(bucket_name, prefix='', max_keys=1000):
    """List objects in a bucket"""
    try:
        response = s3_client.list_objects_v2(
            Bucket=bucket_name,
            Prefix=prefix,
            MaxKeys=max_keys
        )

        if 'Contents' not in response:
            print(f"No objects found in {bucket_name}")
            return []

        print(f"Objects in {bucket_name}:")
        for obj in response['Contents']:
            print(f"  - {obj['Key']} (Size: {obj['Size']}, Modified: {obj['LastModified']})")

        return response['Contents']
    except Exception as e:
        print(f"Error listing objects: {e}")
        raise

# List all objects with pagination
def list_all_objects(bucket_name, prefix=''):
    """List all objects handling pagination"""
    objects = []
    continuation_token = None

    while True:
        try:
            params = {
                'Bucket': bucket_name,
                'Prefix': prefix,
                'MaxKeys': 1000
            }

            if continuation_token:
                params['ContinuationToken'] = continuation_token

            response = s3_client.list_objects_v2(**params)

            if 'Contents' in response:
                objects.extend(response['Contents'])

            if not response.get('IsTruncated'):
                break

            continuation_token = response.get('NextContinuationToken')

        except Exception as e:
            print(f"Error listing objects: {e}")
            raise

    print(f"Total objects: {len(objects)}")
    return objects
```

### Delete Object

```python
def delete_object(bucket_name, key):
    """Delete an object from S3"""
    try:
        response = s3_client.delete_object(
            Bucket=bucket_name,
            Key=key
        )

        print(f"Object '{key}' deleted successfully")
        return response
    except Exception as e:
        print(f"Error deleting object: {e}")
        raise

# Delete multiple objects
def delete_multiple_objects(bucket_name, keys):
    """Delete multiple objects at once"""
    try:
        delete_request = {
            'Objects': [{'Key': key} for key in keys],
            'Quiet': False
        }

        response = s3_client.delete_objects(
            Bucket=bucket_name,
            Delete=delete_request
        )

        if 'Deleted' in response:
            print("Deleted objects:")
            for obj in response['Deleted']:
                print(f"  - {obj['Key']}")

        if 'Errors' in response:
            print("Errors:")
            for err in response['Errors']:
                print(f"  - {err['Key']}: {err['Message']}")

        return response
    except Exception as e:
        print(f"Error deleting objects: {e}")
        raise
```

## Advanced Operations

### Get Object Metadata

```python
def get_object_metadata(bucket_name, key):
    """Get object metadata without downloading content"""
    try:
        response = s3_client.head_object(
            Bucket=bucket_name,
            Key=key
        )

        print("Object Metadata:")
        print(f"  ContentType: {response.get('ContentType')}")
        print(f"  ContentLength: {response.get('ContentLength')}")
        print(f"  LastModified: {response.get('LastModified')}")
        print(f"  ETag: {response.get('ETag')}")
        print(f"  Metadata: {response.get('Metadata', {})}")

        return response
    except s3_client.exceptions.NoSuchKey:
        print(f"Object {key} not found")
        return None
    except Exception as e:
        print(f"Error getting metadata: {e}")
        raise
```

### Copy Object

```python
def copy_object(source_bucket, source_key, dest_bucket, dest_key):
    """Copy object between buckets"""
    try:
        copy_source = {'Bucket': source_bucket, 'Key': source_key}

        response = s3_client.copy_object(
            CopySource=copy_source,
            Bucket=dest_bucket,
            Key=dest_key,
            MetadataDirective='COPY'  # or 'REPLACE' for new metadata
        )

        print(f"Object copied successfully. ETag: {response['CopyObjectResult']['ETag']}")
        return response
    except Exception as e:
        print(f"Error copying object: {e}")
        raise
```

### Generate Presigned URLs

```python
from botocore.exceptions import ClientError

def generate_presigned_url(bucket_name, key, expiration=3600, operation='get_object'):
    """Generate a presigned URL for S3 operations"""
    try:
        response = s3_client.generate_presigned_url(
            operation,
            Params={'Bucket': bucket_name, 'Key': key},
            ExpiresIn=expiration
        )

        print(f"Presigned URL: {response}")
        return response
    except ClientError as e:
        print(f"Error generating presigned URL: {e}")
        raise

# Presigned POST for uploads
def generate_presigned_post(bucket_name, key, expiration=3600):
    """Generate presigned POST data for browser uploads"""
    try:
        response = s3_client.generate_presigned_post(
            Bucket=bucket_name,
            Key=key,
            ExpiresIn=expiration
        )

        print("Presigned POST data:")
        print(f"  URL: {response['url']}")
        print(f"  Fields: {response['fields']}")

        return response
    except ClientError as e:
        print(f"Error generating presigned POST: {e}")
        raise
```

### Bucket Versioning

```python
def enable_versioning(bucket_name):
    """Enable versioning for a bucket"""
    try:
        response = s3_client.put_bucket_versioning(
            Bucket=bucket_name,
            VersioningConfiguration={'Status': 'Enabled'}
        )

        print(f"Versioning enabled for bucket '{bucket_name}'")
        return response
    except Exception as e:
        print(f"Error enabling versioning: {e}")
        raise

def list_object_versions(bucket_name, prefix=''):
    """List all versions of objects"""
    try:
        response = s3_client.list_object_versions(
            Bucket=bucket_name,
            Prefix=prefix
        )

        if 'Versions' in response:
            print("Object versions:")
            for version in response['Versions']:
                print(f"  - {version['Key']} (VersionId: {version['VersionId']}, "
                      f"IsLatest: {version['IsLatest']})")

        return response.get('Versions', [])
    except Exception as e:
        print(f"Error listing versions: {e}")
        raise
```

## Multipart Upload

### Simple Multipart Upload

```python
from boto3.s3.transfer import TransferConfig
import os

def upload_large_file(bucket_name, key, file_path):
    """Upload large file using multipart upload"""

    # Configure multipart threshold and chunk size
    config = TransferConfig(
        multipart_threshold=1024 * 25,  # 25MB
        max_concurrency=10,
        multipart_chunksize=1024 * 25,
        use_threads=True
    )

    # Get file size for progress tracking
    file_size = os.path.getsize(file_path)

    # Progress callback
    class ProgressPercentage:
        def __init__(self, filename):
            self._filename = filename
            self._size = file_size
            self._seen_so_far = 0

        def __call__(self, bytes_amount):
            self._seen_so_far += bytes_amount
            percentage = (self._seen_so_far / self._size) * 100
            print(f"\r{self._filename}: {self._seen_so_far}/{self._size} "
                  f"({percentage:.2f}%)", end='')

    try:
        s3_client.upload_file(
            file_path,
            bucket_name,
            key,
            Config=config,
            Callback=ProgressPercentage(file_path)
        )

        print(f"\nLarge file uploaded successfully to s3://{bucket_name}/{key}")
        return True
    except Exception as e:
        print(f"\nError uploading large file: {e}")
        raise
```

### Manual Multipart Upload

```python
import math

def manual_multipart_upload(bucket_name, key, file_path):
    """Manually handle multipart upload for fine control"""

    # Configuration
    chunk_size = 5 * 1024 * 1024  # 5MB minimum
    file_size = os.path.getsize(file_path)
    chunks_count = math.ceil(file_size / chunk_size)

    try:
        # Initiate multipart upload
        response = s3_client.create_multipart_upload(
            Bucket=bucket_name,
            Key=key
        )
        upload_id = response['UploadId']
        print(f"Multipart upload initiated. UploadId: {upload_id}")

        parts = []

        # Upload parts
        with open(file_path, 'rb') as file:
            for part_number in range(1, chunks_count + 1):
                # Read chunk
                data = file.read(chunk_size)

                # Upload part
                response = s3_client.upload_part(
                    Bucket=bucket_name,
                    Key=key,
                    PartNumber=part_number,
                    UploadId=upload_id,
                    Body=data
                )

                parts.append({
                    'ETag': response['ETag'],
                    'PartNumber': part_number
                })

                print(f"Part {part_number}/{chunks_count} uploaded")

        # Complete multipart upload
        response = s3_client.complete_multipart_upload(
            Bucket=bucket_name,
            Key=key,
            UploadId=upload_id,
            MultipartUpload={'Parts': parts}
        )

        print(f"Multipart upload completed. Location: {response['Location']}")
        return response

    except Exception as e:
        # Abort multipart upload on error
        if 'upload_id' in locals():
            s3_client.abort_multipart_upload(
                Bucket=bucket_name,
                Key=key,
                UploadId=upload_id
            )
            print("Multipart upload aborted")
        raise e
```

## Error Handling

### Comprehensive Error Handler

```python
from botocore.exceptions import ClientError, NoCredentialsError, ParamValidationError

class S3ErrorHandler:
    """Handle S3 errors gracefully"""

    @staticmethod
    def handle_error(func):
        """Decorator for error handling"""
        def wrapper(*args, **kwargs):
            try:
                return func(*args, **kwargs)
            except NoCredentialsError:
                print("Error: AWS credentials not found")
                raise
            except ParamValidationError as e:
                print(f"Error: Invalid parameters - {e}")
                raise
            except ClientError as e:
                error_code = e.response['Error']['Code']
                error_message = e.response['Error']['Message']

                error_handlers = {
                    'NoSuchBucket': 'Bucket does not exist',
                    'NoSuchKey': 'Object does not exist',
                    'BucketAlreadyExists': 'Bucket already exists',
                    'BucketAlreadyOwnedByYou': 'Bucket already owned by you',
                    'AccessDenied': 'Access denied. Check credentials',
                    'InvalidAccessKeyId': 'Invalid access key ID',
                    'SignatureDoesNotMatch': 'Invalid secret access key',
                    'RequestTimeout': 'Request timed out'
                }

                friendly_message = error_handlers.get(
                    error_code,
                    f"Unexpected error: {error_code}"
                )

                print(f"Error: {friendly_message} - {error_message}")
                raise
            except Exception as e:
                print(f"Unexpected error: {e}")
                raise

        return wrapper

# Usage
@S3ErrorHandler.handle_error
def safe_upload(bucket_name, key, file_path):
    return upload_file(bucket_name, key, file_path)
```

### Retry Logic

```python
import time
from typing import Callable, Any

def retry_operation(
    operation: Callable,
    max_retries: int = 3,
    delay: float = 1.0,
    backoff: float = 2.0
) -> Any:
    """Retry an operation with exponential backoff"""

    for attempt in range(max_retries):
        try:
            return operation()
        except Exception as e:
            if attempt == max_retries - 1:
                raise

            wait_time = delay * (backoff ** attempt)
            print(f"Attempt {attempt + 1} failed, retrying in {wait_time:.1f}s...")
            time.sleep(wait_time)

# Usage
result = retry_operation(
    lambda: upload_file('my-bucket', 'file.txt', './file.txt')
)
```

## Async Operations with aioboto3

### Async Configuration

```python
import asyncio
import aioboto3

async def create_async_client():
    """Create async S3 client"""
    session = aioboto3.Session()
    async with session.client(
        's3',
        endpoint_url=os.environ.get('IRONBUCKET_ENDPOINT', 'http://localhost:9000'),
        aws_access_key_id=os.environ.get('IRONBUCKET_ACCESS_KEY'),
        aws_secret_access_key=os.environ.get('IRONBUCKET_SECRET_KEY'),
        region_name=os.environ.get('IRONBUCKET_REGION', 'us-east-1')
    ) as client:
        return client
```

### Async Operations

```python
async def async_upload_file(bucket_name, key, file_path):
    """Async file upload"""
    session = aioboto3.Session()

    async with session.client(
        's3',
        endpoint_url=os.environ.get('IRONBUCKET_ENDPOINT', 'http://localhost:9000'),
        aws_access_key_id=os.environ.get('IRONBUCKET_ACCESS_KEY'),
        aws_secret_access_key=os.environ.get('IRONBUCKET_SECRET_KEY')
    ) as s3:
        with open(file_path, 'rb') as file:
            await s3.put_object(
                Bucket=bucket_name,
                Key=key,
                Body=file
            )
        print(f"Async upload completed: s3://{bucket_name}/{key}")

async def async_batch_upload(bucket_name, files):
    """Upload multiple files concurrently"""
    tasks = []

    for file_info in files:
        task = async_upload_file(
            bucket_name,
            file_info['key'],
            file_info['path']
        )
        tasks.append(task)

    results = await asyncio.gather(*tasks, return_exceptions=True)

    successful = sum(1 for r in results if not isinstance(r, Exception))
    failed = len(results) - successful

    print(f"Batch upload: {successful} successful, {failed} failed")
    return results

# Usage
async def main():
    files = [
        {'key': 'file1.txt', 'path': './file1.txt'},
        {'key': 'file2.txt', 'path': './file2.txt'},
        {'key': 'file3.txt', 'path': './file3.txt'}
    ]

    await async_batch_upload('my-bucket', files)

asyncio.run(main())
```

## Best Practices

### 1. Connection Pooling

```python
from botocore.config import Config

# Configure connection pooling
config = Config(
    region_name='us-east-1',
    signature_version='s3v4',
    retries={
        'max_attempts': 3,
        'mode': 'standard'
    },
    max_pool_connections=50  # Connection pool size
)

s3_client = boto3.client(
    's3',
    endpoint_url=os.environ.get('IRONBUCKET_ENDPOINT', 'http://localhost:9000'),
    aws_access_key_id=os.environ.get('IRONBUCKET_ACCESS_KEY'),
    aws_secret_access_key=os.environ.get('IRONBUCKET_SECRET_KEY'),
    config=config
)
```

### 2. Stream Processing

```python
def process_large_file_stream(bucket_name, key):
    """Process large file without loading into memory"""

    response = s3_client.get_object(Bucket=bucket_name, Key=key)

    # Process line by line for text files
    for line in response['Body'].iter_lines():
        # Process each line
        processed = line.decode('utf-8').upper()
        print(processed)
```

### 3. Batch Operations

```python
from concurrent.futures import ThreadPoolExecutor, as_completed

def batch_download(bucket_name, keys, download_dir):
    """Download multiple files concurrently"""

    def download_single(key):
        local_path = os.path.join(download_dir, key)
        os.makedirs(os.path.dirname(local_path), exist_ok=True)

        s3_client.download_file(bucket_name, key, local_path)
        return key

    with ThreadPoolExecutor(max_workers=10) as executor:
        futures = {
            executor.submit(download_single, key): key
            for key in keys
        }

        for future in as_completed(futures):
            key = futures[future]
            try:
                result = future.result()
                print(f"Downloaded: {result}")
            except Exception as e:
                print(f"Failed to download {key}: {e}")
```

### 4. Memory Efficient Operations

```python
def upload_from_stream(bucket_name, key, stream, chunk_size=8192):
    """Upload from a stream without loading into memory"""

    def read_in_chunks():
        while True:
            data = stream.read(chunk_size)
            if not data:
                break
            yield data

    # Create a file-like object from generator
    from io import BytesIO

    s3_client.put_object(
        Bucket=bucket_name,
        Key=key,
        Body=b''.join(read_in_chunks())
    )
```

## Complete Examples

### Example 1: S3 Sync Tool

```python
import os
import hashlib
from datetime import datetime
from typing import Dict, List, Tuple

class S3SyncTool:
    """Sync local directory with S3 bucket"""

    def __init__(self, s3_client, bucket_name):
        self.s3 = s3_client
        self.bucket = bucket_name

    def sync_to_s3(self, local_dir: str, s3_prefix: str = ''):
        """Sync local directory to S3"""

        local_files = self._get_local_files(local_dir)
        s3_objects = self._get_s3_objects(s3_prefix)

        to_upload, to_delete = self._compare_files(
            local_files, s3_objects, local_dir, s3_prefix
        )

        # Upload new/modified files
        for local_path, s3_key in to_upload:
            print(f"Uploading: {local_path} -> {s3_key}")
            self.s3.upload_file(local_path, self.bucket, s3_key)

        # Delete removed files
        if to_delete:
            delete_request = {
                'Objects': [{'Key': key} for key in to_delete],
                'Quiet': True
            }
            self.s3.delete_objects(
                Bucket=self.bucket,
                Delete=delete_request
            )
            print(f"Deleted {len(to_delete)} objects")

        print(f"Sync complete: {len(to_upload)} uploaded, {len(to_delete)} deleted")

    def _get_local_files(self, local_dir: str) -> Dict[str, str]:
        """Get local files with their MD5 hashes"""
        files = {}

        for root, _, filenames in os.walk(local_dir):
            for filename in filenames:
                filepath = os.path.join(root, filename)
                relative_path = os.path.relpath(filepath, local_dir)

                # Calculate MD5
                with open(filepath, 'rb') as f:
                    md5 = hashlib.md5(f.read()).hexdigest()

                files[relative_path] = md5

        return files

    def _get_s3_objects(self, prefix: str) -> Dict[str, str]:
        """Get S3 objects with their ETags"""
        objects = {}

        paginator = self.s3.get_paginator('list_objects_v2')
        pages = paginator.paginate(
            Bucket=self.bucket,
            Prefix=prefix
        )

        for page in pages:
            if 'Contents' in page:
                for obj in page['Contents']:
                    # Remove prefix and quotes from ETag
                    key = obj['Key'][len(prefix):].lstrip('/')
                    etag = obj['ETag'].strip('"')
                    objects[key] = etag

        return objects

    def _compare_files(
        self,
        local_files: Dict[str, str],
        s3_objects: Dict[str, str],
        local_dir: str,
        s3_prefix: str
    ) -> Tuple[List[Tuple[str, str]], List[str]]:
        """Compare local and S3 files"""

        to_upload = []
        to_delete = []

        # Find files to upload
        for local_path, local_md5 in local_files.items():
            s3_key = os.path.join(s3_prefix, local_path).replace('\\', '/')

            if local_path not in s3_objects or local_md5 != s3_objects[local_path]:
                full_local_path = os.path.join(local_dir, local_path)
                to_upload.append((full_local_path, s3_key))

        # Find objects to delete
        for s3_path in s3_objects:
            if s3_path not in local_files:
                s3_key = os.path.join(s3_prefix, s3_path).replace('\\', '/')
                to_delete.append(s3_key)

        return to_upload, to_delete

# Usage
sync_tool = S3SyncTool(s3_client, 'my-bucket')
sync_tool.sync_to_s3('./local-folder', 'remote-folder/')
```

### Example 2: S3 Backup Manager

```python
import json
import gzip
from datetime import datetime, timedelta

class S3BackupManager:
    """Manage backups in S3 with compression and rotation"""

    def __init__(self, s3_client, bucket_name):
        self.s3 = s3_client
        self.bucket = bucket_name

    def backup_directory(self, directory: str, backup_prefix: str):
        """Create compressed backup of directory"""

        timestamp = datetime.now().strftime('%Y%m%d_%H%M%S')
        backup_key = f"{backup_prefix}/backup_{timestamp}.tar.gz"

        # Create tar.gz archive
        import tarfile
        import tempfile

        with tempfile.NamedTemporaryFile(suffix='.tar.gz') as tmp_file:
            with tarfile.open(tmp_file.name, 'w:gz') as tar:
                tar.add(directory, arcname=os.path.basename(directory))

            # Upload to S3
            self.s3.upload_file(
                tmp_file.name,
                self.bucket,
                backup_key,
                ExtraArgs={
                    'Metadata': {
                        'backup-date': timestamp,
                        'source-directory': directory
                    }
                }
            )

        print(f"Backup created: s3://{self.bucket}/{backup_key}")
        return backup_key

    def rotate_backups(self, backup_prefix: str, retention_days: int):
        """Delete backups older than retention period"""

        cutoff_date = datetime.now() - timedelta(days=retention_days)

        # List all backups
        response = self.s3.list_objects_v2(
            Bucket=self.bucket,
            Prefix=backup_prefix
        )

        if 'Contents' not in response:
            return

        to_delete = []

        for obj in response['Contents']:
            if obj['LastModified'].replace(tzinfo=None) < cutoff_date:
                to_delete.append({'Key': obj['Key']})

        if to_delete:
            self.s3.delete_objects(
                Bucket=self.bucket,
                Delete={'Objects': to_delete, 'Quiet': True}
            )
            print(f"Deleted {len(to_delete)} old backups")

    def restore_backup(self, backup_key: str, restore_path: str):
        """Restore backup from S3"""

        import tarfile
        import tempfile

        with tempfile.NamedTemporaryFile(suffix='.tar.gz') as tmp_file:
            # Download backup
            self.s3.download_file(
                self.bucket,
                backup_key,
                tmp_file.name
            )

            # Extract archive
            with tarfile.open(tmp_file.name, 'r:gz') as tar:
                tar.extractall(restore_path)

        print(f"Backup restored to {restore_path}")

# Usage
backup_manager = S3BackupManager(s3_client, 'backup-bucket')

# Create backup
backup_manager.backup_directory('/important/data', 'daily-backups')

# Rotate old backups
backup_manager.rotate_backups('daily-backups', retention_days=30)

# Restore backup
backup_manager.restore_backup('daily-backups/backup_20240101_120000.tar.gz', '/restore/here')
```

### Example 3: S3 Data Pipeline

```python
import pandas as pd
from io import StringIO, BytesIO

class S3DataPipeline:
    """Process data files in S3"""

    def __init__(self, s3_client, bucket_name):
        self.s3 = s3_client
        self.bucket = bucket_name

    def process_csv_file(self, input_key: str, output_key: str):
        """Process CSV file in S3"""

        # Read CSV from S3
        response = self.s3.get_object(Bucket=self.bucket, Key=input_key)
        df = pd.read_csv(response['Body'])

        print(f"Loaded {len(df)} rows from {input_key}")

        # Process data
        df = self._transform_data(df)

        # Write back to S3
        csv_buffer = StringIO()
        df.to_csv(csv_buffer, index=False)

        self.s3.put_object(
            Bucket=self.bucket,
            Key=output_key,
            Body=csv_buffer.getvalue(),
            ContentType='text/csv'
        )

        print(f"Processed data saved to {output_key}")

    def _transform_data(self, df: pd.DataFrame) -> pd.DataFrame:
        """Transform data (example)"""
        # Add timestamp
        df['processed_at'] = datetime.now()

        # Clean data
        df = df.dropna()

        # Add derived columns
        if 'value' in df.columns:
            df['value_squared'] = df['value'] ** 2

        return df

    def batch_process_files(self, prefix: str, output_prefix: str):
        """Process multiple files"""

        # List files to process
        response = self.s3.list_objects_v2(
            Bucket=self.bucket,
            Prefix=prefix
        )

        if 'Contents' not in response:
            print("No files to process")
            return

        for obj in response['Contents']:
            if obj['Key'].endswith('.csv'):
                input_key = obj['Key']
                output_key = input_key.replace(prefix, output_prefix)

                try:
                    self.process_csv_file(input_key, output_key)
                except Exception as e:
                    print(f"Error processing {input_key}: {e}")

# Usage
pipeline = S3DataPipeline(s3_client, 'data-bucket')
pipeline.batch_process_files('raw-data/', 'processed-data/')
```

## Performance Tips

1. **Use multipart uploads** for files > 100MB
2. **Enable transfer acceleration** for global uploads
3. **Use concurrent operations** with ThreadPoolExecutor
4. **Stream large files** instead of loading into memory
5. **Implement connection pooling** for better performance
6. **Use batch operations** to reduce API calls
7. **Cache frequently accessed objects** locally
8. **Compress data** before uploading when appropriate

## Troubleshooting

### Common Issues

1. **Connection Refused**
   ```python
   # Check endpoint URL and port
   # Verify IronBucket is running
   # Check firewall settings
   ```

2. **Invalid Credentials**
   ```python
   # Verify access key and secret key
   # Check if credentials are properly configured
   # Ensure region is set (even if arbitrary)
   ```

3. **Slow Performance**
   ```python
   # Use multipart upload for large files
   # Enable concurrent operations
   # Check network connectivity
   # Increase connection pool size
   ```

4. **Memory Issues**
   ```python
   # Use streaming for large files
   # Process data in chunks
   # Implement pagination for listings
   # Use generators instead of lists
   ```

## Testing

### Unit Testing with moto

```python
import unittest
from moto import mock_s3
import boto3

@mock_s3
class TestS3Operations(unittest.TestCase):

    def setUp(self):
        """Set up test fixtures"""
        self.s3 = boto3.client('s3', region_name='us-east-1')
        self.bucket_name = 'test-bucket'
        self.s3.create_bucket(Bucket=self.bucket_name)

    def test_upload_file(self):
        """Test file upload"""
        key = 'test-file.txt'
        content = b'Hello, World!'

        self.s3.put_object(
            Bucket=self.bucket_name,
            Key=key,
            Body=content
        )

        # Verify upload
        response = self.s3.get_object(
            Bucket=self.bucket_name,
            Key=key
        )

        self.assertEqual(response['Body'].read(), content)

    def test_list_objects(self):
        """Test object listing"""
        # Upload test objects
        for i in range(5):
            self.s3.put_object(
                Bucket=self.bucket_name,
                Key=f'file-{i}.txt',
                Body=f'Content {i}'.encode()
            )

        # List objects
        response = self.s3.list_objects_v2(
            Bucket=self.bucket_name
        )

        self.assertEqual(len(response['Contents']), 5)

if __name__ == '__main__':
    unittest.main()
```

## Additional Resources

- [boto3 Documentation](https://boto3.amazonaws.com/v1/documentation/api/latest/index.html)
- [S3 API Reference](https://docs.aws.amazon.com/AmazonS3/latest/API/)
- [IronBucket API Documentation](./API.md)
- [IronBucket Configuration Guide](../README.md#configuration)