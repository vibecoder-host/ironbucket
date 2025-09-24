# IronBucket
High-performance S3-compatible storage server written in Rust, optimized for speed and reliability.

<div align="left">

<img src="ironbucket-icon.jpg" alt="IronBucket Logo" width="300" />

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL%20v3-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)

</div>

## Features

- **S3 API Compatibility**: Complete implementation of core S3 operations
  - Bucket operations: Create, Delete, List, Head
  - Object operations: PUT, GET, DELETE, HEAD
  - Multipart uploads: Initiate, Upload Parts, Complete, Abort
  - Query operations: Versioning, ACL, Location, Batch Delete
- **AWS Signature V4**: Complete authentication implementation
- **Chunked Transfer Encoding**: Full support for AWS chunked transfers with signatures
- **Async I/O**: Built on Tokio and Axum for maximum concurrency
- **Disk Persistence**: Reliable filesystem-based storage
- **CORS Support**: Full cross-origin resource sharing support
- **Zero-Copy Operations**: Efficient memory usage for large files
- **Exceptional Performance**: 20,000+ operations per second

Also check the Web UI here: https://github.com/vibecoder-host/ironbucket-ui

## Performance Metrics

<details>
<summary><b>📊 View Benchmark Results</b> (Tool: MinIO Warp, Server: 8 cores / 16GB RAM)</summary>

```bash
./warp mixed --host=172.17.0.1:20000 --access-key=XXX --secret-key=XXX \
    --obj.size=100KB --duration=60s --autoterm
```

### 1KB Files
- **Total Throughput**: 28,561 obj/s | 16.71 MB/s (mixed workload)
- **PUT Operations**: 4,284 obj/s | 4.09 MB/s
- **GET Operations**: 12,852 obj/s | 12.26 MB/s
- **DELETE Operations**: 2,856 obj/s
- **STAT Operations**: 8,568 obj/s
- **Latency**: < 2ms average response time (P50), 15ms (P99)

### 10KB Files
- **Total Throughput**: 26,627 obj/s | 152.34 MB/s (mixed workload)
- **PUT Operations**: 3,993 obj/s | 38.08 MB/s
- **GET Operations**: 11,981 obj/s | 114.26 MB/s
- **DELETE Operations**: 2,663 obj/s
- **STAT Operations**: 7,989 obj/s
- **Latency**: < 2ms average response time (P50), 18ms (P99)

### 100KB Files
- **Total Throughput**: 19,307 obj/s | 1104.77 MB/s (mixed workload)
- **PUT Operations**: 2,896 obj/s | 276.23 MB/s
- **GET Operations**: 8,687 obj/s | 828.54 MB/s
- **DELETE Operations**: 1,930 obj/s
- **STAT Operations**: 5,792 obj/s
- **Latency**: < 3ms average response time (P50), 24ms (P99)

### 1MB Files
- **Total Throughput**: 5,067 obj/s | 2.898 GB/s (mixed workload)
- **PUT Operations**: 759 obj/s | 724.29 MB/s
- **GET Operations**: 2,280 obj/s | 2.18 GB/s
- **DELETE Operations**: 507 obj/s
- **STAT Operations**: 1,520 obj/s
- **Latency**: < 5ms average response time (P50), 36ms (P99)

### 10MB Files
- **Total Throughput**: 735 obj/s | 4.23 GB/s (mixed workload)
- **PUT Operations**: 110 obj/s | 1049.50 MB/s
- **GET Operations**: 330 obj/s | 3.16 GB/s
- **DELETE Operations**: 73 obj/s
- **STAT Operations**: 221 obj/s
- **Latency**: < 25ms average response time (P50), 140ms (P99)
                    

</details>  
  

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


## Documentation

Comprehensive documentation is available in the `/doc` folder:

### Setup & Configuration
- [**Installation Guide**](doc/INSTALL.md) - Complete installation instructions for various platforms
- [**Configuration Guide**](doc/CONFIGURATION.md) - Detailed configuration options and environment variables
- [**Security Guide**](doc/SECURITY.md) - Security best practices and authentication setup

### Usage Guides
- [**API Reference**](doc/API.md) - Complete S3 API endpoint documentation
- [**CLI Usage**](doc/USAGE_CLI.md) - Command-line interface guide and examples
- [**Node.js SDK**](doc/USAGE_NODEJS.md) - Node.js integration and AWS SDK usage
- [**Python SDK**](doc/USAGE_PYTHON.md) - Python boto3 integration guide
- [**Rust SDK**](doc/USAGE_RUST.md) - Rust AWS SDK integration examples

### Operations & Maintenance
- [**Performance Guide**](doc/PERFORMANCE.md) - Performance tuning and optimization tips
- [**Troubleshooting**](doc/TROUBLESHOOTING.md) - Common issues and solutions
- [**Documentation Index**](doc/README.md) - Overview of all documentation

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