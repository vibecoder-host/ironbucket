# IronBucket

High-performance S3-compatible storage server written in Rust, optimized for speed and reliability.

## Features

- **Full S3 API Compatibility**: Complete implementation of core S3 operations
  - Bucket operations: Create, Delete, List, Head
  - Object operations: PUT, GET, DELETE, HEAD
  - Multipart uploads: Initiate, Upload Parts, Complete, Abort
  - Query operations: Versioning, ACL, Location, Batch Delete
- **Exceptional Performance**: 20,000+ operations per second
- **Chunked Transfer Encoding**: Full support for AWS chunked transfers with signatures
- **Async I/O**: Built on Tokio and Axum for maximum concurrency
- **Disk Persistence**: Reliable filesystem-based storage
- **Redis Integration**: Optional caching layer for enhanced performance
- **AWS Signature V4**: Complete authentication implementation
- **CORS Support**: Full cross-origin resource sharing support
- **Zero-Copy Operations**: Efficient memory usage for large files

## Performance Metrics

Benchmarked with MinIO warp on standard hardware (8 cores / 16GB ram):

`  	./warp mixed --host=172.17.0.1:20000 --access-key=XXX --secret-key=XXX \
    --obj.size=100KB --duration=60s --autoterm
`

1KB files:
  - Total Throughput: 20,164 obj/s | 11.54 MB/s (mixed workload)
  - PUT Operations: 3,563 obj/s | 2.88 MB/s
  - GET Operations: 9,073 obj/s | 8.65 MB/s
  - DELETE Operations: 2,016 obj/s
  - STAT Operations: 5,127 obj/s
  - Latency: < 3ms average response time (P50), 8ms (P99)
  - Concurrency: Handles 20+ concurrent connections efficiently

10KB files:
- Total Throughput: 19,476 obj/s | 111.43 MB/s (mixed workload)
  - PUT Operations: 2,920 obj/s | 27.85 MB/s
  - GET Operations: 8,764 obj/s | 83.58 MB/s
  - DELETE Operations: 1,947 obj/s
  - STAT Operations: 5,844 obj/s
  - Latency: < 3ms average response time (P50), 10ms (P99)
  - Concurrency: Handles 20+ concurrent connections efficiently

100KB files:
  - Total Throughput: 14,296 obj/s | 818.28 MB/s (mixed workload)
  - PUT Operations: 2,144 obj/s | 204.52 MB/s
  - GET Operations: 6,432 obj/s | 613.49 MB/s
  - DELETE Operations: 1,429 obj/s
  - STAT Operations: 4,129 obj/s
  - Latency: < 4ms average response time (P50), 25ms (P99)
  - Concurrency: Handles 20+ concurrent connections efficiently
  
  
 1MB files:
  - Total Throughput: 4,671 obj/s | 2672.35 MB/s (mixed workload)
  - PUT Operations: 701 obj/s | 668.66 MB/s
  - GET Operations: 2,101 obj/s | 2003.69 MB/s
  - DELETE Operations: 467 obj/s
  - STAT Operations: 1,401 obj/s
  - Latency: < 15ms average response time (P50), 44ms (P99)
  - Concurrency: Handles 20+ concurrent connections efficiently
  
  
 10MB files:
  - Total Throughput: 543 obj/s | 3117.43 MB/s (mixed workload)
  - PUT Operations: 82 obj/s | 782.53 MB/s
  - GET Operations: 245 obj/s | 2334.90 MB/s
  - DELETE Operations: 54 obj/s
  - STAT Operations: 162 obj/s
  - Latency: < 50ms average response time (P50), 229ms (P99)
  - Concurrency: Handles 20+ concurrent connections efficiently  
  

## Quick Start

### Using Docker (Recommended)

```bash
# Clone the repository
cd /opt/app/ironbucket

# Start IronBucket with Docker Compose
docker-compose up -d

# Verify it's running
docker-compose ps

# Check logs
docker-compose logs -f ironbucket
```

### Using Cargo

```bash
# Build from source
cargo build --release

# Run with environment variables
STORAGE_PATH=/s3 ./target/release/ironbucket
```

## Configuration

Configuration via environment variables or `.env` file:

```env
# Storage
STORAGE_PATH=/s3                    # Directory for object storage
MAX_FILE_SIZE=5368709120            # Max file size (5GB default)

# Server
PORT=9000                           # Server port
RUST_LOG=ironbucket=info          # Logging level

# Authentication (S3 compatible)
ACCESS_KEY=root
SECRET_KEY=xxxxxxxxxxxxxxxxxxxxx

```

## Docker Compose Configuration

```yaml
services:
  ironbucket:
    build: .
    ports:
      - "172.17.0.1:20000:9000"
    volumes:
      - ./s3:/s3
    environment:
      - STORAGE_PATH=/s3
      - RUST_LOG=ironbucket=warn,tower_http=warn
    restart: always
```

## API Endpoints

### Bucket Operations

| Operation | Endpoint | Description |
|-----------|----------|-------------|
| List Buckets | `GET /` | List all buckets |
| Create Bucket | `PUT /{bucket}` | Create a new bucket |
| Delete Bucket | `DELETE /{bucket}` | Delete an empty bucket |
| Head Bucket | `HEAD /{bucket}` | Check if bucket exists |
| List Objects | `GET /{bucket}` | List objects in bucket |
| Get Location | `GET /{bucket}?location` | Get bucket location |
| Get Versioning | `GET /{bucket}?versioning` | Get versioning status |
| Get ACL | `GET /{bucket}?acl` | Get bucket ACL |
| List Uploads | `GET /{bucket}?uploads` | List multipart uploads |
| Batch Delete | `POST /{bucket}?delete` | Delete multiple objects |

### Object Operations

| Operation | Endpoint | Description |
|-----------|----------|-------------|
| Put Object | `PUT /{bucket}/{key}` | Upload an object |
| Get Object | `GET /{bucket}/{key}` | Download an object |
| Delete Object | `DELETE /{bucket}/{key}` | Delete an object |
| Head Object | `HEAD /{bucket}/{key}` | Get object metadata |
| Get Object ACL | `GET /{bucket}/{key}?acl` | Get object ACL |

### Multipart Upload Operations

| Operation | Endpoint | Description |
|-----------|----------|-------------|
| Initiate Upload | `POST /{bucket}/{key}?uploads` | Start multipart upload |
| Upload Part | `PUT /{bucket}/{key}?partNumber=N&uploadId=ID` | Upload a part |
| Complete Upload | `POST /{bucket}/{key}?uploadId=ID` | Complete multipart upload |
| Abort Upload | `DELETE /{bucket}/{key}?uploadId=ID` | Abort multipart upload |
| List Parts | `GET /{bucket}/{key}?uploadId=ID` | List uploaded parts |

## Testing with AWS CLI

```bash
# Configure credentials
export AWS_ACCESS_KEY_ID=root
export AWS_SECRET_ACCESS_KEY=xxxxxxxxxxxxxxxxxxxxx
export AWS_ENDPOINT=http://172.17.0.1:20000

# Create a bucket
aws --endpoint-url $AWS_ENDPOINT s3 mb s3://my-bucket

# Upload a file
aws --endpoint-url $AWS_ENDPOINT s3 cp file.txt s3://my-bucket/

# List objects
aws --endpoint-url $AWS_ENDPOINT s3 ls s3://my-bucket/

# Download a file
aws --endpoint-url $AWS_ENDPOINT s3 cp s3://my-bucket/file.txt ./downloaded.txt

# Delete a file
aws --endpoint-url $AWS_ENDPOINT s3 rm s3://my-bucket/file.txt

# Remove bucket
aws --endpoint-url $AWS_ENDPOINT s3 rb s3://my-bucket
```

## Benchmarking

### Using MinIO Warp benchmark tool

```bash
# Download warp
wget https://github.com/minio/warp/releases/download/v0.7.11/warp_0.7.11_Linux_x86_64.tar.gz
tar -xzf warp_0.7.11_Linux_x86_64.tar.gz

# Run mixed benchmark
./warp mixed \
  --host=localhost:20000 \
  --access-key=root \
  --secret-key=xxxxxxxxxxxxxxxxxxxxx \
  --autoterm \
  --duration=60s \
  --concurrent=50

# Run specific operation benchmarks
./warp get --host=localhost:20000 ...  # Test GET performance
./warp put --host=localhost:20000 ...  # Test PUT performance
./warp delete --host=localhost:20000 ... # Test DELETE performance
```

### Key Components

- **Axum Web Framework**: High-performance async HTTP server
- **Tokio Runtime**: Async I/O and task scheduling
- **Chunked Transfer Parser**: Handles AWS chunked encoding with signatures
- **Storage Layer**: Direct filesystem operations with optional caching
- **Auth Middleware**: AWS Signature V4 validation

## Storage Management

```bash
# Check storage usage
du -sh /opt/app/ironbucket/s3/

# Clean up all storage
rm -rf /opt/app/ironbucket/s3/*

```

## Monitoring

```bash
# View logs
docker-compose logs -f ironbucket

# Check container status
docker-compose ps

# Monitor performance
docker stats ironbucket
```

## Development

```bash
# Run in development mode
RUST_LOG=debug cargo run

# Run tests
cargo test

# Format code
cargo fmt

# Check for issues
cargo clippy
```

## Troubleshooting

### Port Already in Use
```bash
# Check what's using port 20000
netstat -tlnp | grep 20000

# Stop IronBucket
docker-compose down
```

### Storage Permission Issues
```bash
# Fix permissions
sudo chown -R $USER:$USER /opt/app/ironbucket/s3
```

### Clear All Data
```bash
# Stop services
docker-compose down

# Clear storage
rm -rf s3/* redis-data/*

# Restart
docker-compose up -d
```

## Recently completed
- [x] Object versioning support (Completed)
- [x] Bucket policies and IAM integration (Completed)
- [x] Server-side encryption (Completed - AES-256-GCM)
- [x] CORS configuration support (Completed)
- [x] Object lifecycle management

## Future Enhancements
- [ ] Bucket analytics and metrics
- [ ] Replication
- [ ] Event notifications


## Contributing

Contributions are welcome! Please ensure:
1. Code follows Rust best practices
2. All tests pass
3. Performance benchmarks show no regression
4. Documentation is updated


---

## Support

- **GitHub Issues**: [Report bugs](https://github.com/vibecoder-host/ironbucket/issues)
- **Discussions**: [Ask questions](https://github.com/vibecoder-host/ironbucket/discussions)
- **Security**: [Report vulnerabilities](SECURITY.md)


---

## License

This project is licensed under the GNU Affero General Public License v3.0 (AGPL-3.0) - see the LICENSE file for details.