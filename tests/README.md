# IronBucket Test Suite

Comprehensive test suite for IronBucket S3-compatible storage server.

## Quick Start

```bash
# Copy environment configuration
cp .env.example .env

# Edit .env with your configuration
vim .env

# Run all tests
./run-all-tests.sh

# Or run individual tests
./test-s3-operations.sh
./test-metadata-persistence.sh
./test-performance.sh
```

## Prerequisites

Required tools:
- AWS CLI (`apt-get install awscli`)
- jq (`apt-get install jq`)
- curl (`apt-get install curl`)
- Docker/Docker Compose

IronBucket must be running:
```bash
cd /opt/app/ironbucket
docker compose up -d ironbucket
```

## Configuration

All test configuration is managed through environment variables in `.env`:

```bash
# S3 endpoint configuration
S3_ENDPOINT=http://172.17.0.1:20000
S3_ACCESS_KEY=admin
S3_SECRET_KEY=admin
S3_REGION=us-east-1

# Test configuration
TEST_BUCKET_PREFIX=test
STORAGE_PATH=/opt/app/ironbucket/s3

# Performance test configuration
WARP_DURATION=30s
WARP_OBJECT_SIZE=10KB
WARP_CONCURRENCY=20
```

## Available Tests

### 1. Basic S3 Operations (`test-s3-operations.sh`)

Tests fundamental S3 functionality:
- Bucket operations (create, list, delete)
- Object upload and download
- Object listing with prefixes
- Object deletion
- Large file uploads
- Special characters in object keys

```bash
./test-s3-operations.sh
```

### 2. Metadata Persistence (`test-metadata-persistence.sh`)

Tests metadata storage and retrieval:
- Metadata file creation
- Content-type preservation
- HEAD request metadata
- Metadata persistence after restart
- Metadata format validation

```bash
./test-metadata-persistence.sh
```

### 3. Performance Testing (`test-performance.sh`)

Benchmarks using MinIO's warp tool:
- Mixed workload (PUT/GET/DELETE/STAT)
- PUT performance
- GET performance
- Throughput measurements
- Latency statistics

```bash
./test-performance.sh
```

## Test Utilities

The `test-utils.sh` file provides common functions for all tests:

- `load_test_env()` - Load environment variables from .env
- `check_dependencies()` - Verify required tools are installed
- `check_ironbucket_running()` - Ensure IronBucket is accessible
- `create_test_bucket()` - Create a test bucket with unique name
- `cleanup_test_bucket()` - Remove test bucket and its contents
- `upload_test_file()` - Upload a file with specified content-type
- `check_metadata_exists()` - Verify metadata file exists
- `get_metadata_content()` - Read metadata JSON content
- `run_test()` - Execute a test with formatted output
- `print_summary()` - Display test results summary

## Running All Tests

Use the provided script to run all tests sequentially:

```bash
#!/bin/bash
./run-all-tests.sh
```

This will execute:
1. S3 operations tests
2. Metadata persistence tests
3. Performance tests (optional)

## Test Output

Tests use color-coded output for clarity:
- 🟢 Green: Test passed
- 🔴 Red: Test failed
- 🟡 Yellow: Test in progress or warning

Example output:
```
Testing Basic S3 Operations
===========================
✓ Loaded configuration from .env
✓ IronBucket is running at http://172.17.0.1:20000

▶ Bucket create/list/delete
  ✓ Bucket operations successful
✓ Bucket create/list/delete passed

═══════════════════════════════════════
Test Summary
═══════════════════════════════════════
✓ All tests passed! (6/6)
═══════════════════════════════════════
```

## CI/CD Integration

For automated testing in CI/CD pipelines:

```bash
# Set environment variables
export S3_ENDPOINT=http://localhost:9000
export S3_ACCESS_KEY=testkey
export S3_SECRET_KEY=testsecret

# Run tests with exit code
./run-all-tests.sh
if [ $? -eq 0 ]; then
    echo "All tests passed"
else
    echo "Tests failed"
    exit 1
fi
```

## Troubleshooting

### IronBucket not accessible
```bash
# Check if container is running
docker compose ps

# Check logs
docker compose logs ironbucket

# Restart IronBucket
docker compose restart ironbucket
```

### Authentication failures
Ensure credentials in `.env` match IronBucket configuration:
```bash
grep access_keys /opt/app/ironbucket/src/main.rs
```

### Missing dependencies
Install required tools:
```bash
apt-get update
apt-get install -y awscli jq curl
```

### Performance test failures
For warp benchmark issues, try smaller object sizes:
```bash
# Edit .env
WARP_OBJECT_SIZE=1KB  # Use smaller objects
```

## Adding New Tests

To add a new test:

1. Create a new test script in the `tests/` directory
2. Source the test utilities:
   ```bash
   source "$(dirname "$0")/test-utils.sh"
   ```
3. Use the provided utility functions
4. Follow the naming convention: `test-<feature>.sh`
5. Update this README with test documentation

## License

See main IronBucket LICENSE file.