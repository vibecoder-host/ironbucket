# Configuration Guide

This guide covers all configuration options available in IronBucket, including environment variables, configuration files, and runtime parameters.

## Table of Contents

- [Configuration Methods](#configuration-methods)
- [Environment Variables](#environment-variables)
- [Configuration File](#configuration-file)
- [Core Settings](#core-settings)
- [Storage Configuration](#storage-configuration)
- [Authentication](#authentication)
- [Network Settings](#network-settings)
- [Performance Tuning](#performance-tuning)
- [Logging](#logging)
- [Redis Cache](#redis-cache)
- [Security Settings](#security-settings)
- [Advanced Configuration](#advanced-configuration)
- [Configuration Examples](#configuration-examples)

## Configuration Methods

IronBucket supports multiple configuration methods, with the following precedence order:

1. **Command-line arguments** (highest priority)
2. **Environment variables**
3. **Configuration file**
4. **Default values** (lowest priority)

### Command-line Arguments

```bash
ironbucket \
  --host 0.0.0.0 \
  --port 9000 \
  --storage-path /data/s3 \
  --access-key admin \
  --secret-key admin \
  --log-level info
```

### Environment Variables

```bash
export HOST=0.0.0.0
export PORT=9000
export STORAGE_PATH=/data/s3
export ACCESS_KEY=admin
export SECRET_KEY=admin
export RUST_LOG=ironbucket=info
```

### Configuration File

Create a `config.toml` file:

```toml
[server]
host = "0.0.0.0"
port = 9000

[storage]
path = "/data/s3"
max_file_size = 5368709120

[auth]
access_key = "admin"
secret_key = "admin"

[logging]
level = "info"
```

## Environment Variables

### Complete List

| Variable | Description | Default | Example |
|----------|-------------|---------|---------|
| `HOST` | Server bind address | `0.0.0.0` | `127.0.0.1` |
| `PORT` | Server port | `9000` | `8080` |
| `STORAGE_PATH` | Object storage directory | `/data/s3` | `/var/lib/ironbucket` |
| `MAX_FILE_SIZE` | Maximum file size (bytes) | `5368709120` (5GB) | `10737418240` |
| `ACCESS_KEY` | S3 access key ID | `admin` | `AKIAIOSFODNN7EXAMPLE` |
| `SECRET_KEY` | S3 secret access key | `admin` | `wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY` |
| `REGION` | AWS region | `us-east-1` | `eu-west-1` |
| `RUST_LOG` | Logging configuration | `ironbucket=info` | `ironbucket=debug,tower_http=debug` |
| `REDIS_URL` | Redis connection URL | None | `redis://localhost:6379` |
| `REDIS_HOST` | Redis host (alternative) | `localhost` | `redis.example.com` |
| `REDIS_PORT` | Redis port (alternative) | `6379` | `6380` |
| `REDIS_PASSWORD` | Redis password | None | `secretpassword` |
| `REDIS_DB` | Redis database number | `0` | `1` |
| `ENABLE_VERSIONING` | Enable object versioning | `false` | `true` |
| `ENABLE_ENCRYPTION` | Enable server-side encryption | `false` | `true` |
| `ENCRYPTION_KEY` | Encryption master key | Auto-generated | `base64encodedkey` |
| `CORS_ENABLED` | Enable CORS support | `true` | `false` |
| `CORS_ORIGINS` | Allowed CORS origins | `*` | `https://example.com,https://app.example.com` |
| `METRICS_ENABLED` | Enable metrics endpoint | `false` | `true` |
| `METRICS_PORT` | Metrics server port | `9090` | `9091` |

## Configuration File

### TOML Format

IronBucket supports a TOML configuration file. By default, it looks for `config.toml` in the following locations:

1. Current directory
2. `/etc/ironbucket/`
3. `$HOME/.config/ironbucket/`
4. Path specified by `--config` flag

Example `config.toml`:

```toml
# Server Configuration
[server]
host = "0.0.0.0"
port = 9000
workers = 4  # Number of worker threads (0 = number of CPU cores)
max_connections = 1000
keep_alive = 60  # seconds
request_timeout = 300  # seconds

# Storage Configuration
[storage]
path = "/data/s3"
max_file_size = 5368709120  # 5GB in bytes
temp_dir = "/tmp/ironbucket"
enable_compression = false
compression_level = 6  # 1-9, higher = better compression

# Authentication
[auth]
access_key = "admin"
secret_key = "admin"
enable_anonymous = false
session_timeout = 3600  # seconds

# Advanced Authentication
[[auth.users]]
access_key = "user1"
secret_key = "password1"
permissions = ["read", "write"]
buckets = ["bucket1", "bucket2"]

[[auth.users]]
access_key = "readonly"
secret_key = "readonlypass"
permissions = ["read"]
buckets = ["*"]  # All buckets

# Network Configuration
[network]
enable_ipv6 = true
tcp_nodelay = true
tcp_keepalive = 7200  # seconds
buffer_size = 65536  # bytes

# Logging
[logging]
level = "info"  # trace, debug, info, warn, error
format = "json"  # text, json
file = "/var/log/ironbucket/server.log"
max_size = "100MB"
max_backups = 10
max_age = 30  # days

# Redis Cache
[redis]
enabled = true
url = "redis://localhost:6379"
password = ""
db = 0
pool_size = 10
timeout = 5  # seconds
ttl = 3600  # seconds

# Security
[security]
enable_tls = false
cert_file = "/etc/ironbucket/cert.pem"
key_file = "/etc/ironbucket/key.pem"
enable_encryption = true
encryption_algorithm = "AES256"
enable_audit_log = true
audit_log_file = "/var/log/ironbucket/audit.log"

# CORS Configuration
[cors]
enabled = true
allowed_origins = ["*"]
allowed_methods = ["GET", "POST", "PUT", "DELETE", "HEAD", "OPTIONS"]
allowed_headers = ["*"]
exposed_headers = ["ETag", "x-amz-version-id"]
max_age = 86400  # seconds
allow_credentials = false

# Versioning
[versioning]
enabled = true
max_versions = 100
default_retention = 2592000  # 30 days in seconds

# Lifecycle Management
[lifecycle]
enabled = true
scan_interval = 3600  # seconds
worker_threads = 2

# Metrics
[metrics]
enabled = true
port = 9090
path = "/metrics"
include_histogram = true

# Rate Limiting
[rate_limit]
enabled = true
requests_per_second = 100
burst_size = 200
per_ip = true

# Replication
[replication]
enabled = false
mode = "async"  # sync, async
targets = ["http://backup1.example.com:9000", "http://backup2.example.com:9000"]
retry_attempts = 3
retry_delay = 5  # seconds
```

## Core Settings

### Server Configuration

```bash
# Bind to specific interface
export HOST=192.168.1.100

# Use custom port
export PORT=8080

# Set worker threads (0 = auto-detect)
export WORKERS=8

# Connection limits
export MAX_CONNECTIONS=5000
export KEEP_ALIVE_TIMEOUT=120
```

### Storage Paths

```bash
# Primary storage location
export STORAGE_PATH=/mnt/storage/s3

# Temporary files location
export TEMP_DIR=/mnt/fast-storage/tmp

# Metadata storage
export METADATA_PATH=/var/lib/ironbucket/metadata
```

## Storage Configuration

### File Size Limits

```bash
# Maximum single file size (10GB)
export MAX_FILE_SIZE=10737418240

# Multipart upload part size (10MB)
export MULTIPART_CHUNK_SIZE=10485760

# Minimum multipart size (5MB)
export MULTIPART_MIN_SIZE=5242880
```

### Storage Optimization

```bash
# Enable compression for stored objects
export ENABLE_COMPRESSION=true
export COMPRESSION_LEVEL=6  # 1-9

# Enable deduplication
export ENABLE_DEDUP=true

# Storage class support
export STORAGE_CLASSES="STANDARD,INFREQUENT_ACCESS,GLACIER"
```

## Authentication

### Basic Authentication

```bash
# Single user mode
export ACCESS_KEY=myaccesskey
export SECRET_KEY=mysecretkey

# Disable authentication (NOT RECOMMENDED)
export ENABLE_AUTH=false
```

### Multi-User Configuration

Create a `users.json` file:

```json
{
  "users": [
    {
      "access_key": "admin",
      "secret_key": "adminpass",
      "role": "admin",
      "buckets": ["*"]
    },
    {
      "access_key": "user1",
      "secret_key": "user1pass",
      "role": "user",
      "buckets": ["bucket1", "bucket2"]
    },
    {
      "access_key": "readonly",
      "secret_key": "readonlypass",
      "role": "readonly",
      "buckets": ["public-*"]
    }
  ]
}
```

Load users:

```bash
export AUTH_CONFIG_FILE=/etc/ironbucket/users.json
```

### IAM Policy Support

```bash
# Enable IAM policies
export ENABLE_IAM=true

# Policy file location
export IAM_POLICY_FILE=/etc/ironbucket/policies.json
```

## Network Settings

### TCP Configuration

```bash
# TCP optimization
export TCP_NODELAY=true
export TCP_KEEPALIVE=7200
export SO_REUSEADDR=true

# Buffer sizes
export RECV_BUFFER_SIZE=262144  # 256KB
export SEND_BUFFER_SIZE=262144  # 256KB
```

### HTTP Settings

```bash
# Request limits
export MAX_REQUEST_SIZE=5368709120  # 5GB
export REQUEST_TIMEOUT=300  # seconds
export IDLE_TIMEOUT=60  # seconds

# Header limits
export MAX_HEADER_SIZE=8192  # bytes
export MAX_HEADERS=100
```

## Performance Tuning

### Concurrency

```bash
# Worker threads
export WORKERS=16  # 0 for auto

# Async runtime threads
export ASYNC_THREADS=8

# Blocking thread pool
export BLOCKING_THREADS=512
```

### Memory Management

```bash
# Memory limits
export MAX_MEMORY_CACHE=1073741824  # 1GB

# Buffer pool settings
export BUFFER_POOL_SIZE=100
export BUFFER_SIZE=65536  # 64KB

# GC settings (if applicable)
export MALLOC_ARENA_MAX=2
```

### File System

```bash
# File descriptor limit
ulimit -n 65536

# Directory listing cache
export DIR_CACHE_TTL=60  # seconds
export DIR_CACHE_SIZE=1000  # entries
```

## Logging

### Log Levels

```bash
# Set global log level
export RUST_LOG=info

# Module-specific levels
export RUST_LOG=ironbucket=debug,tower_http=info,hyper=warn

# Verbose debugging
export RUST_LOG=trace
export RUST_BACKTRACE=1
```

### Log Formats

```bash
# JSON format for structured logging
export LOG_FORMAT=json

# Pretty format for development
export LOG_FORMAT=pretty

# Compact format for production
export LOG_FORMAT=compact
```

### Log Output

```bash
# Log to file
export LOG_FILE=/var/log/ironbucket/server.log

# Log rotation
export LOG_MAX_SIZE=100MB
export LOG_MAX_BACKUPS=10
export LOG_MAX_AGE=30  # days

# Separate error log
export ERROR_LOG_FILE=/var/log/ironbucket/error.log
```

## Redis Cache

### Connection Settings

```bash
# Redis URL (takes precedence)
export REDIS_URL=redis://username:password@localhost:6379/0

# Alternative configuration
export REDIS_HOST=localhost
export REDIS_PORT=6379
export REDIS_PASSWORD=secretpassword
export REDIS_DB=0
```

### Cache Configuration

```bash
# Enable/disable cache
export ENABLE_CACHE=true

# Cache TTL settings
export CACHE_TTL=3600  # seconds
export METADATA_CACHE_TTL=300
export LIST_CACHE_TTL=60

# Cache size limits
export MAX_CACHE_SIZE=1073741824  # 1GB
export MAX_CACHE_ENTRIES=10000
```

### Redis Pool

```bash
# Connection pool settings
export REDIS_POOL_SIZE=20
export REDIS_POOL_MIN_IDLE=5
export REDIS_CONNECTION_TIMEOUT=5  # seconds
export REDIS_IDLE_TIMEOUT=300  # seconds
```

## Security Settings

### TLS/SSL

```bash
# Enable HTTPS
export ENABLE_TLS=true
export TLS_CERT=/etc/ironbucket/cert.pem
export TLS_KEY=/etc/ironbucket/key.pem

# TLS options
export TLS_MIN_VERSION=TLS1.2
export TLS_CIPHERS=TLS_AES_256_GCM_SHA384:TLS_AES_128_GCM_SHA256
```

### Encryption

```bash
# Server-side encryption
export ENABLE_ENCRYPTION=true
export ENCRYPTION_ALGORITHM=AES256-GCM

# Master key (base64 encoded)
export ENCRYPTION_MASTER_KEY=base64encodedmasterkey

# Key management
export KMS_ENABLED=true
export KMS_ENDPOINT=https://kms.example.com
```

### Access Control

```bash
# IP filtering
export ALLOWED_IPS=192.168.1.0/24,10.0.0.0/8
export DENIED_IPS=192.168.1.100,192.168.1.101

# Bucket policies
export ENABLE_BUCKET_POLICIES=true
export DEFAULT_BUCKET_POLICY=private  # private, public-read, public-read-write
```

## Advanced Configuration

### Versioning

```bash
# Object versioning
export ENABLE_VERSIONING=true
export MAX_VERSIONS_PER_OBJECT=100
export VERSION_CLEANUP_INTERVAL=86400  # seconds

# Soft delete
export ENABLE_SOFT_DELETE=true
export SOFT_DELETE_RETENTION=604800  # 7 days
```

### Lifecycle Management

```bash
# Enable lifecycle rules
export ENABLE_LIFECYCLE=true
export LIFECYCLE_SCAN_INTERVAL=3600  # 1 hour

# Default expiration
export DEFAULT_EXPIRATION_DAYS=90
```

### Replication

```bash
# Enable replication
export ENABLE_REPLICATION=true
export REPLICATION_MODE=async  # sync or async

# Target endpoints
export REPLICATION_TARGETS=http://backup1:9000,http://backup2:9000

# Replication settings
export REPLICATION_WORKERS=4
export REPLICATION_QUEUE_SIZE=1000
export REPLICATION_RETRY_ATTEMPTS=3
```

### Monitoring

```bash
# Metrics endpoint
export METRICS_ENABLED=true
export METRICS_PORT=9090
export METRICS_PATH=/metrics

# Health check
export HEALTH_CHECK_PATH=/health
export HEALTH_CHECK_INTERVAL=30  # seconds

# Tracing
export ENABLE_TRACING=true
export JAEGER_ENDPOINT=http://jaeger:14268/api/traces
```

## Configuration Examples

### Development Environment

```bash
# .env.development
HOST=127.0.0.1
PORT=9000
STORAGE_PATH=./dev-data
ACCESS_KEY=dev
SECRET_KEY=devpassword
RUST_LOG=ironbucket=debug,tower_http=debug
ENABLE_AUTH=false
CORS_ENABLED=true
CORS_ORIGINS=*
```

### Production Environment

```bash
# .env.production
HOST=0.0.0.0
PORT=9000
STORAGE_PATH=/mnt/storage/s3
ACCESS_KEY=${SECRET_ACCESS_KEY}
SECRET_KEY=${SECRET_SECRET_KEY}
RUST_LOG=ironbucket=warn,tower_http=warn
ENABLE_TLS=true
TLS_CERT=/etc/ssl/certs/ironbucket.crt
TLS_KEY=/etc/ssl/private/ironbucket.key
ENABLE_ENCRYPTION=true
REDIS_URL=redis://redis-cluster:6379
METRICS_ENABLED=true
ENABLE_AUDIT_LOG=true
RATE_LIMIT_ENABLED=true
```

### High-Performance Setup

```bash
# .env.performance
WORKERS=32
ASYNC_THREADS=16
BLOCKING_THREADS=1024
MAX_CONNECTIONS=10000
KEEP_ALIVE_TIMEOUT=300
TCP_NODELAY=true
RECV_BUFFER_SIZE=524288  # 512KB
SEND_BUFFER_SIZE=524288  # 512KB
ENABLE_CACHE=true
REDIS_POOL_SIZE=50
MAX_MEMORY_CACHE=4294967296  # 4GB
DIR_CACHE_SIZE=5000
```

### Docker Compose Configuration

```yaml
version: '3.8'

services:
  ironbucket:
    image: ironbucket:latest
    environment:
      - HOST=0.0.0.0
      - PORT=9000
      - STORAGE_PATH=/data/s3
      - ACCESS_KEY=${ACCESS_KEY:-admin}
      - SECRET_KEY=${SECRET_KEY:-admin}
      - RUST_LOG=${LOG_LEVEL:-info}
      - REDIS_URL=redis://redis:6379
      - ENABLE_METRICS=true
      - METRICS_PORT=9090
    volumes:
      - ./data:/data/s3
      - ./config/config.toml:/etc/ironbucket/config.toml:ro
    ports:
      - "9000:9000"
      - "9090:9090"
    depends_on:
      - redis

  redis:
    image: redis:7-alpine
    volumes:
      - ./redis-data:/data
    command: redis-server --appendonly yes
```

### Kubernetes ConfigMap

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: ironbucket-config
data:
  config.toml: |
    [server]
    host = "0.0.0.0"
    port = 9000
    workers = 8

    [storage]
    path = "/data/s3"
    max_file_size = 10737418240

    [auth]
    access_key = "admin"
    secret_key = "admin"

    [redis]
    enabled = true
    url = "redis://redis-service:6379"

    [metrics]
    enabled = true
    port = 9090
```

## Validation

### Configuration Testing

```bash
# Test configuration without starting server
ironbucket --config /etc/ironbucket/config.toml --test-config

# Validate environment variables
ironbucket --validate-env

# Check effective configuration
ironbucket --print-config
```

### Health Checks

```bash
# Basic health check
curl http://localhost:9000/health

# Detailed health check
curl http://localhost:9000/health?detailed=true

# Readiness check
curl http://localhost:9000/ready
```

## Migration

### From MinIO

```bash
# MinIO compatibility mode
export MINIO_COMPATIBILITY=true
export MINIO_ACCESS_KEY=$MINIO_ACCESS_KEY
export MINIO_SECRET_KEY=$MINIO_SECRET_KEY

# Use MinIO's data directory
export STORAGE_PATH=/data/minio
```

### From AWS S3

```bash
# AWS compatibility mode
export AWS_COMPATIBILITY=true
export AWS_REGION=us-east-1
export ENABLE_SIGNATURE_V4=true
export ENABLE_CHUNKED_ENCODING=true
```

## Best Practices

1. **Use environment variables** for sensitive data (access keys, passwords)
2. **Use configuration files** for complex setups
3. **Enable logging** with appropriate levels for your environment
4. **Set resource limits** based on your hardware
5. **Enable monitoring** for production deployments
6. **Use Redis cache** for improved performance
7. **Configure backups** and replication for data safety
8. **Implement rate limiting** to prevent abuse
9. **Enable audit logging** for compliance
10. **Regularly rotate** access keys and certificates

## Troubleshooting

### Debug Configuration Issues

```bash
# Enable trace logging
export RUST_LOG=trace
export RUST_BACKTRACE=full

# Log all configuration sources
export CONFIG_DEBUG=true

# Test specific components
export TEST_STORAGE=true
export TEST_AUTH=true
export TEST_REDIS=true
```

### Common Problems

1. **Port binding errors**: Check if port is already in use
2. **Permission denied**: Ensure proper file/directory permissions
3. **Redis connection failed**: Verify Redis is running and accessible
4. **High memory usage**: Adjust cache sizes and worker counts
5. **Slow performance**: Enable caching and tune worker settings

## Next Steps

- [Installation Guide](./INSTALL.md) - Installation instructions
- [API Documentation](./API.md) - Complete API reference
- [Performance Tuning](./PERFORMANCE.md) - Optimization guide
- [Security Guide](./SECURITY.md) - Security best practices