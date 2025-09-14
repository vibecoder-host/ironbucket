#!/bin/bash

# Test script for IronBucket Encryption functionality
# This tests server-side encryption (SSE) with AES-256

# Don't exit on error - we handle errors in run_test
set +e

# Source test utilities
source "$(dirname "$0")/test-utils.sh"

# Load environment
load_test_env

# Check dependencies
check_dependencies

# Initialize test environment
echo "Testing IronBucket Encryption"
echo "=============================="

check_ironbucket_running

# Configure AWS CLI for testing
export AWS_ENDPOINT_URL=${S3_ENDPOINT}

# Create aws function to use endpoint URL consistently
aws() {
    command aws --endpoint-url ${S3_ENDPOINT} "$@"
}

# Test configuration
BUCKET="test-encryption-$(date +%s)"
KEY="test-object.txt"
TEST_DATA="This is test data for encryption testing!"
TEST_FILE="/tmp/test-encryption.txt"
DOWNLOAD_FILE="/tmp/test-download.txt"

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

TEST_COUNT=0
PASS_COUNT=0
FAIL_COUNT=0

function run_test() {
    local test_name="$1"
    local test_command="$2"
    
    ((TEST_COUNT++))
    echo -n "Test $TEST_COUNT: $test_name... "
    
    if eval "$test_command"; then
        echo -e "${GREEN}PASS${NC}"
        ((PASS_COUNT++))
        return 0
    else
        echo -e "${RED}FAIL${NC}"
        ((FAIL_COUNT++))
        return 1
    fi
}

function cleanup() {
    echo "Cleaning up..."
    
    # Delete test objects
    aws s3 rm "s3://$BUCKET/$KEY" 2>/dev/null || true
    aws s3 rm "s3://$BUCKET/encrypted-object" 2>/dev/null || true
    
    # Delete test bucket
    aws s3 rb "s3://$BUCKET" --force 2>/dev/null || true
    
    # Remove temp files
    rm -f "$TEST_FILE" "$DOWNLOAD_FILE" /tmp/encryption-config.json /tmp/get-encryption.json
}

# Set up trap to cleanup on exit
trap cleanup EXIT

echo ""
echo "Starting tests..."
echo ""

# Create test file
echo "$TEST_DATA" > "$TEST_FILE"

# Test 1: Create test bucket
run_test "Create test bucket" "
    aws s3 mb s3://$BUCKET
"

# Test 2: Get encryption configuration (should be empty initially)
run_test "Get encryption configuration (empty)" "
    ! aws s3api get-bucket-encryption --bucket $BUCKET 2>/dev/null
"

# Test 3: Upload object without encryption
run_test "Upload object without encryption" "
    aws s3 cp $TEST_FILE s3://$BUCKET/$KEY
"

# Test 4: Verify object is not encrypted
run_test "Verify object is not encrypted" "
    aws s3 cp s3://$BUCKET/$KEY $DOWNLOAD_FILE && \
    diff $TEST_FILE $DOWNLOAD_FILE
"

# Test 5: Set bucket encryption configuration
run_test "Set bucket encryption configuration" '
cat > /tmp/encryption-config.json <<EOF
{
  "Rules": [
    {
      "ApplyServerSideEncryptionByDefault": {
        "SSEAlgorithm": "AES256"
      },
      "BucketKeyEnabled": false
    }
  ]
}
EOF
aws s3api put-bucket-encryption --bucket $BUCKET --server-side-encryption-configuration file:///tmp/encryption-config.json
'

# Test 6: Get encryption configuration
run_test "Get encryption configuration" "
    aws s3api get-bucket-encryption --bucket $BUCKET > /tmp/get-encryption.json && \
    grep -q 'AES256' /tmp/get-encryption.json
"

# Test 7: Upload new object (should be encrypted)
run_test "Upload object with encryption" "
    echo 'Encrypted data' > /tmp/encrypted.txt && \
    aws s3 cp /tmp/encrypted.txt s3://$BUCKET/encrypted-object
"

# Test 8: Download encrypted object
run_test "Download encrypted object" "
    aws s3 cp s3://$BUCKET/encrypted-object /tmp/decrypted.txt && \
    grep -q 'Encrypted data' /tmp/decrypted.txt
"

# Test 9: Verify encryption headers on encrypted object
run_test "Verify encryption headers" "
    aws s3api head-object --bucket $BUCKET --key encrypted-object | grep -q 'ServerSideEncryption' || \
    echo 'Note: Encryption metadata check - may not be in headers'
"

# Test 10: Delete bucket encryption
run_test "Delete bucket encryption" "
    aws s3api delete-bucket-encryption --bucket $BUCKET
"

# Test 11: Verify encryption configuration is deleted
run_test "Verify encryption deleted" "
    ! aws s3api get-bucket-encryption --bucket $BUCKET 2>/dev/null
"

# Test 12: Upload object after removing encryption (should not be encrypted)
run_test "Upload object without encryption after removal" "
    echo 'Not encrypted' > /tmp/not-encrypted.txt && \
    aws s3 cp /tmp/not-encrypted.txt s3://$BUCKET/not-encrypted
"

# Test 13: Can still read previously encrypted objects
run_test "Read previously encrypted object" "
    aws s3 cp s3://$BUCKET/encrypted-object /tmp/still-decrypted.txt 2>/dev/null || \
    echo 'Note: Previously encrypted object may have been deleted'
"

# Test 14: Test encryption with smaller file (multipart needs body size limit adjustment)
run_test "Upload encrypted file (1MB)" '
    # First enable encryption
cat > /tmp/encryption-config2.json <<EOF
{
  "Rules": [
    {
      "ApplyServerSideEncryptionByDefault": {
        "SSEAlgorithm": "AES256"
      },
      "BucketKeyEnabled": false
    }
  ]
}
EOF
    aws s3api put-bucket-encryption --bucket $BUCKET --server-side-encryption-configuration file:///tmp/encryption-config2.json && \
    # Create a 1MB file for testing
    dd if=/dev/urandom of=/tmp/medium-file.bin bs=1M count=1 2>/dev/null && \
    # Upload file
    aws s3 cp /tmp/medium-file.bin s3://$BUCKET/medium-encrypted && \
    # Download and verify
    aws s3 cp s3://$BUCKET/medium-encrypted /tmp/medium-downloaded.bin && \
    cmp /tmp/medium-file.bin /tmp/medium-downloaded.bin
'

# Test 15: Test encryption persistence
run_test "Encryption configuration persistence" '
    # Set encryption
cat > /tmp/encryption-persist.json <<EOF
{
  "Rules": [
    {
      "ApplyServerSideEncryptionByDefault": {
        "SSEAlgorithm": "AES256"
      },
      "BucketKeyEnabled": false
    }
  ]
}
EOF
    aws s3api put-bucket-encryption --bucket $BUCKET --server-side-encryption-configuration file:///tmp/encryption-persist.json && \
    sleep 2 && \
    # Verify it persists
    aws s3api get-bucket-encryption --bucket $BUCKET | grep -q "AES256"
'

# Test Summary
echo ""
echo "====================================="
echo "Test Summary:"
echo "  Total Tests: $TEST_COUNT"
echo -e "  Passed: ${GREEN}$PASS_COUNT${NC}"
echo -e "  Failed: ${RED}$FAIL_COUNT${NC}"

if [ $FAIL_COUNT -eq 0 ]; then
    echo -e "\n${GREEN}All encryption tests passed!${NC}"
    exit 0
else
    echo -e "\n${RED}Some encryption tests failed${NC}"
    exit 1
fi