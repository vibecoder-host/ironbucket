#!/bin/bash

# Test script for IronBucket encryption module functionality
# This tests the ring-based encryption implementation in src/encryption.rs

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test configuration
TEST_DIR="/tmp/ironbucket-encryption-test-$$"
BUCKET_NAME="test-encryption-bucket-$$"
OBJECT_KEY="test-object"
PORT=9876
S3_ENDPOINT="http://localhost:$PORT"
ACCESS_KEY="test_access_key"
SECRET_KEY="test_secret_key"

# Test counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# Cleanup function
cleanup() {
    echo -e "\n${BLUE}Cleaning up...${NC}"

    # Stop the server
    if [ ! -z "$SERVER_PID" ]; then
        kill $SERVER_PID 2>/dev/null || true
        wait $SERVER_PID 2>/dev/null || true
    fi

    # Clean up test directory
    rm -rf "$TEST_DIR"

    # Clean up AWS CLI config
    rm -f ~/.aws/credentials.test
    rm -f ~/.aws/config.test
}

# Register cleanup on exit
trap cleanup EXIT

# Test helper functions
assert_equals() {
    local expected="$1"
    local actual="$2"
    local test_name="$3"

    TESTS_RUN=$((TESTS_RUN + 1))

    if [ "$expected" == "$actual" ]; then
        echo -e "${GREEN}✓${NC} $test_name"
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        echo -e "${RED}✗${NC} $test_name"
        echo -e "  Expected: $expected"
        echo -e "  Actual: $actual"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

assert_contains() {
    local substring="$1"
    local string="$2"
    local test_name="$3"

    TESTS_RUN=$((TESTS_RUN + 1))

    if echo "$string" | grep -q "$substring"; then
        echo -e "${GREEN}✓${NC} $test_name"
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        echo -e "${RED}✗${NC} $test_name"
        echo -e "  Expected to contain: $substring"
        echo -e "  Actual: $string"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

assert_not_contains() {
    local substring="$1"
    local string="$2"
    local test_name="$3"

    TESTS_RUN=$((TESTS_RUN + 1))

    if ! echo "$string" | grep -q "$substring"; then
        echo -e "${GREEN}✓${NC} $test_name"
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        echo -e "${RED}✗${NC} $test_name"
        echo -e "  Expected NOT to contain: $substring"
        echo -e "  Actual: $string"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

assert_success() {
    local exit_code="$1"
    local test_name="$2"

    TESTS_RUN=$((TESTS_RUN + 1))

    if [ "$exit_code" -eq 0 ]; then
        echo -e "${GREEN}✓${NC} $test_name"
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        echo -e "${RED}✗${NC} $test_name (exit code: $exit_code)"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

assert_failure() {
    local exit_code="$1"
    local test_name="$2"

    TESTS_RUN=$((TESTS_RUN + 1))

    if [ "$exit_code" -ne 0 ]; then
        echo -e "${GREEN}✓${NC} $test_name"
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        echo -e "${RED}✗${NC} $test_name (expected failure but got success)"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

assert_file_exists() {
    local file="$1"
    local test_name="$2"

    TESTS_RUN=$((TESTS_RUN + 1))

    if [ -f "$file" ]; then
        echo -e "${GREEN}✓${NC} $test_name"
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        echo -e "${RED}✗${NC} $test_name"
        echo -e "  File does not exist: $file"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

# Setup test environment
setup_test_env() {
    echo -e "\n${BLUE}Setting up test environment...${NC}"

    # Create test directory
    mkdir -p "$TEST_DIR"

    # Create AWS CLI config for testing
    mkdir -p ~/.aws
    cat > ~/.aws/credentials.test << EOF
[default]
aws_access_key_id = $ACCESS_KEY
aws_secret_access_key = $SECRET_KEY
EOF

    cat > ~/.aws/config.test << EOF
[default]
region = us-east-1
output = json
EOF

    export AWS_SHARED_CREDENTIALS_FILE=~/.aws/credentials.test
    export AWS_CONFIG_FILE=~/.aws/config.test
}

# Start the server with encryption enabled
start_server_with_encryption() {
    local enable_encryption="${1:-false}"
    local encryption_key="${2:-}"

    echo -e "\n${BLUE}Starting IronBucket server with encryption=$enable_encryption...${NC}"

    # Build the server
    cd /opt/app/ironbucket
    cargo build --release 2>/dev/null

    # Start server with environment variables
    if [ -n "$encryption_key" ]; then
        ENABLE_ENCRYPTION="$enable_encryption" \
        ENCRYPTION_KEY="$encryption_key" \
        STORAGE_PATH="$TEST_DIR" \
        BIND_ADDRESS="127.0.0.1:$PORT" \
        AWS_ACCESS_KEY_ID="$ACCESS_KEY" \
        AWS_SECRET_ACCESS_KEY="$SECRET_KEY" \
        ./target/release/ironbucket > /dev/null 2>&1 &
    else
        ENABLE_ENCRYPTION="$enable_encryption" \
        STORAGE_PATH="$TEST_DIR" \
        BIND_ADDRESS="127.0.0.1:$PORT" \
        AWS_ACCESS_KEY_ID="$ACCESS_KEY" \
        AWS_SECRET_ACCESS_KEY="$SECRET_KEY" \
        ./target/release/ironbucket > /dev/null 2>&1 &
    fi

    SERVER_PID=$!

    # Wait for server to start
    sleep 3

    # Check if server is running
    if ! kill -0 $SERVER_PID 2>/dev/null; then
        echo -e "${RED}Failed to start IronBucket server${NC}"
        exit 1
    fi

    echo -e "${GREEN}Server started with PID $SERVER_PID${NC}"
}

# Stop the server
stop_server() {
    if [ ! -z "$SERVER_PID" ]; then
        echo -e "\n${BLUE}Stopping server...${NC}"
        kill $SERVER_PID 2>/dev/null || true
        wait $SERVER_PID 2>/dev/null || true
        SERVER_PID=""
    fi
}

# Main test execution
main() {
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}IronBucket Encryption Module Tests${NC}"
    echo -e "${BLUE}========================================${NC}"

    setup_test_env

    # Test 1: Server starts without encryption
    echo -e "\n${YELLOW}Test Suite 1: Server without encryption${NC}"
    start_server_with_encryption "false"

    # Create bucket
    aws --endpoint-url=$S3_ENDPOINT s3 mb s3://$BUCKET_NAME 2>/dev/null
    assert_success $? "Create bucket without encryption enabled"

    # Upload object
    echo "Test data without encryption" > $TEST_DIR/test-file.txt
    aws --endpoint-url=$S3_ENDPOINT s3 cp $TEST_DIR/test-file.txt s3://$BUCKET_NAME/$OBJECT_KEY 2>/dev/null
    assert_success $? "Upload object without encryption"

    # Download and verify
    aws --endpoint-url=$S3_ENDPOINT s3 cp s3://$BUCKET_NAME/$OBJECT_KEY $TEST_DIR/downloaded.txt 2>/dev/null
    assert_success $? "Download object without encryption"

    content=$(cat $TEST_DIR/downloaded.txt)
    assert_equals "Test data without encryption" "$content" "Content matches without encryption"

    # Check raw storage - should be plaintext
    if [ -f "$TEST_DIR/$BUCKET_NAME/$OBJECT_KEY" ]; then
        raw_content=$(cat "$TEST_DIR/$BUCKET_NAME/$OBJECT_KEY")
        assert_equals "Test data without encryption" "$raw_content" "Raw storage is plaintext without encryption"
    fi

    # Cleanup
    aws --endpoint-url=$S3_ENDPOINT s3 rb s3://$BUCKET_NAME --force 2>/dev/null
    rm -f $TEST_DIR/test-file.txt $TEST_DIR/downloaded.txt

    stop_server

    # Test 2: Server with global encryption enabled
    echo -e "\n${YELLOW}Test Suite 2: Server with global encryption enabled${NC}"
    start_server_with_encryption "true"

    # Create bucket
    aws --endpoint-url=$S3_ENDPOINT s3 mb s3://$BUCKET_NAME 2>/dev/null
    assert_success $? "Create bucket with global encryption enabled"

    # Upload object
    echo "Test data with encryption" > $TEST_DIR/test-file.txt
    aws --endpoint-url=$S3_ENDPOINT s3 cp $TEST_DIR/test-file.txt s3://$BUCKET_NAME/$OBJECT_KEY 2>/dev/null
    assert_success $? "Upload object with global encryption"

    # Download and verify
    aws --endpoint-url=$S3_ENDPOINT s3 cp s3://$BUCKET_NAME/$OBJECT_KEY $TEST_DIR/downloaded.txt 2>/dev/null
    assert_success $? "Download object with global encryption"

    content=$(cat $TEST_DIR/downloaded.txt)
    assert_equals "Test data with encryption" "$content" "Content matches with global encryption"

    # Cleanup
    aws --endpoint-url=$S3_ENDPOINT s3 rb s3://$BUCKET_NAME --force 2>/dev/null
    rm -f $TEST_DIR/test-file.txt $TEST_DIR/downloaded.txt

    stop_server

    # Test 3: Server with encryption key from environment
    echo -e "\n${YELLOW}Test Suite 3: Server with encryption key from environment${NC}"
    # Generate a base64-encoded 256-bit key
    ENCRYPTION_KEY=$(openssl rand -base64 32)
    start_server_with_encryption "true" "$ENCRYPTION_KEY"

    # Create bucket
    aws --endpoint-url=$S3_ENDPOINT s3 mb s3://$BUCKET_NAME 2>/dev/null
    assert_success $? "Create bucket with encryption key"

    # Upload object
    echo "Test data with specific key" > $TEST_DIR/test-file.txt
    aws --endpoint-url=$S3_ENDPOINT s3 cp $TEST_DIR/test-file.txt s3://$BUCKET_NAME/$OBJECT_KEY 2>/dev/null
    assert_success $? "Upload object with encryption key"

    # Download and verify
    aws --endpoint-url=$S3_ENDPOINT s3 cp s3://$BUCKET_NAME/$OBJECT_KEY $TEST_DIR/downloaded.txt 2>/dev/null
    assert_success $? "Download object with encryption key"

    content=$(cat $TEST_DIR/downloaded.txt)
    assert_equals "Test data with specific key" "$content" "Content matches with encryption key"

    # Cleanup
    aws --endpoint-url=$S3_ENDPOINT s3 rb s3://$BUCKET_NAME --force 2>/dev/null
    rm -f $TEST_DIR/test-file.txt $TEST_DIR/downloaded.txt

    stop_server

    # Test 4: Bucket-level encryption configuration
    echo -e "\n${YELLOW}Test Suite 4: Bucket-level encryption configuration${NC}"
    start_server_with_encryption "false"

    # Create bucket
    aws --endpoint-url=$S3_ENDPOINT s3 mb s3://$BUCKET_NAME 2>/dev/null
    assert_success $? "Create bucket for encryption configuration"

    # Set bucket encryption
    cat > $TEST_DIR/encryption-config.json << 'EOF'
{
    "Rules": [
        {
            "ApplyServerSideEncryptionByDefault": {
                "SSEAlgorithm": "AES256"
            },
            "BucketKeyEnabled": true
        }
    ]
}
EOF

    aws --endpoint-url=$S3_ENDPOINT s3api put-bucket-encryption \
        --bucket $BUCKET_NAME \
        --server-side-encryption-configuration file://$TEST_DIR/encryption-config.json 2>/dev/null
    assert_success $? "Set bucket encryption configuration"

    # Get bucket encryption
    aws --endpoint-url=$S3_ENDPOINT s3api get-bucket-encryption \
        --bucket $BUCKET_NAME > $TEST_DIR/get-encryption.json 2>/dev/null
    assert_success $? "Get bucket encryption configuration"

    # Verify configuration
    if [ -f $TEST_DIR/get-encryption.json ]; then
        assert_contains "AES256" "$(cat $TEST_DIR/get-encryption.json)" "Encryption configuration contains AES256"
    fi

    # Upload object to encrypted bucket
    echo "Test data in encrypted bucket" > $TEST_DIR/test-file.txt
    aws --endpoint-url=$S3_ENDPOINT s3 cp $TEST_DIR/test-file.txt s3://$BUCKET_NAME/$OBJECT_KEY 2>/dev/null
    assert_success $? "Upload object to encrypted bucket"

    # Download and verify
    aws --endpoint-url=$S3_ENDPOINT s3 cp s3://$BUCKET_NAME/$OBJECT_KEY $TEST_DIR/downloaded.txt 2>/dev/null
    assert_success $? "Download object from encrypted bucket"

    content=$(cat $TEST_DIR/downloaded.txt)
    assert_equals "Test data in encrypted bucket" "$content" "Content matches from encrypted bucket"

    # Delete bucket encryption
    aws --endpoint-url=$S3_ENDPOINT s3api delete-bucket-encryption \
        --bucket $BUCKET_NAME 2>/dev/null
    assert_success $? "Delete bucket encryption configuration"

    # Verify deletion
    aws --endpoint-url=$S3_ENDPOINT s3api get-bucket-encryption \
        --bucket $BUCKET_NAME > $TEST_DIR/no-encryption.json 2>&1
    assert_failure $? "Get bucket encryption after deletion should fail"

    # Cleanup
    aws --endpoint-url=$S3_ENDPOINT s3 rb s3://$BUCKET_NAME --force 2>/dev/null
    rm -f $TEST_DIR/test-file.txt $TEST_DIR/downloaded.txt

    stop_server

    # Test 5: Large file encryption
    echo -e "\n${YELLOW}Test Suite 5: Large file encryption${NC}"
    start_server_with_encryption "true"

    # Create bucket
    aws --endpoint-url=$S3_ENDPOINT s3 mb s3://$BUCKET_NAME 2>/dev/null
    assert_success $? "Create bucket for large file"

    # Create a 10MB file
    dd if=/dev/urandom of=$TEST_DIR/large-file.bin bs=1M count=10 2>/dev/null
    assert_success $? "Create 10MB test file"

    # Calculate checksum of original
    original_checksum=$(md5sum $TEST_DIR/large-file.bin | cut -d' ' -f1)

    # Upload large file
    aws --endpoint-url=$S3_ENDPOINT s3 cp $TEST_DIR/large-file.bin s3://$BUCKET_NAME/large-object 2>/dev/null
    assert_success $? "Upload 10MB encrypted file"

    # Download and verify checksum
    aws --endpoint-url=$S3_ENDPOINT s3 cp s3://$BUCKET_NAME/large-object $TEST_DIR/downloaded-large.bin 2>/dev/null
    assert_success $? "Download 10MB encrypted file"

    downloaded_checksum=$(md5sum $TEST_DIR/downloaded-large.bin | cut -d' ' -f1)
    assert_equals "$original_checksum" "$downloaded_checksum" "Large file checksum matches after encryption/decryption"

    # Cleanup
    aws --endpoint-url=$S3_ENDPOINT s3 rb s3://$BUCKET_NAME --force 2>/dev/null
    rm -f $TEST_DIR/large-file.bin $TEST_DIR/downloaded-large.bin

    stop_server

    # Test 6: Mixed encrypted and unencrypted objects
    echo -e "\n${YELLOW}Test Suite 6: Mixed encrypted and unencrypted objects${NC}"
    start_server_with_encryption "false"

    # Create bucket
    aws --endpoint-url=$S3_ENDPOINT s3 mb s3://$BUCKET_NAME 2>/dev/null
    assert_success $? "Create bucket for mixed encryption"

    # Upload unencrypted object
    echo "Unencrypted object" > $TEST_DIR/unencrypted.txt
    aws --endpoint-url=$S3_ENDPOINT s3 cp $TEST_DIR/unencrypted.txt s3://$BUCKET_NAME/unencrypted 2>/dev/null
    assert_success $? "Upload unencrypted object"

    # Enable bucket encryption
    aws --endpoint-url=$S3_ENDPOINT s3api put-bucket-encryption \
        --bucket $BUCKET_NAME \
        --server-side-encryption-configuration file://$TEST_DIR/encryption-config.json 2>/dev/null
    assert_success $? "Enable bucket encryption after upload"

    # Upload encrypted object
    echo "Encrypted object" > $TEST_DIR/encrypted.txt
    aws --endpoint-url=$S3_ENDPOINT s3 cp $TEST_DIR/encrypted.txt s3://$BUCKET_NAME/encrypted 2>/dev/null
    assert_success $? "Upload encrypted object"

    # Download both objects
    aws --endpoint-url=$S3_ENDPOINT s3 cp s3://$BUCKET_NAME/unencrypted $TEST_DIR/downloaded-unencrypted.txt 2>/dev/null
    assert_success $? "Download unencrypted object from mixed bucket"

    aws --endpoint-url=$S3_ENDPOINT s3 cp s3://$BUCKET_NAME/encrypted $TEST_DIR/downloaded-encrypted.txt 2>/dev/null
    assert_success $? "Download encrypted object from mixed bucket"

    # Verify content
    unencrypted_content=$(cat $TEST_DIR/downloaded-unencrypted.txt)
    assert_equals "Unencrypted object" "$unencrypted_content" "Unencrypted object content matches"

    encrypted_content=$(cat $TEST_DIR/downloaded-encrypted.txt)
    assert_equals "Encrypted object" "$encrypted_content" "Encrypted object content matches"

    # Cleanup
    aws --endpoint-url=$S3_ENDPOINT s3 rb s3://$BUCKET_NAME --force 2>/dev/null

    stop_server

    # Print summary
    echo -e "\n${BLUE}========================================${NC}"
    echo -e "${BLUE}Test Summary${NC}"
    echo -e "${BLUE}========================================${NC}"
    echo -e "Total tests run: $TESTS_RUN"
    echo -e "${GREEN}Tests passed: $TESTS_PASSED${NC}"
    if [ $TESTS_FAILED -gt 0 ]; then
        echo -e "${RED}Tests failed: $TESTS_FAILED${NC}"
    else
        echo -e "${GREEN}Tests failed: $TESTS_FAILED${NC}"
    fi

    if [ $TESTS_FAILED -gt 0 ]; then
        echo -e "\n${RED}SOME TESTS FAILED${NC}"
        exit 1
    else
        echo -e "\n${GREEN}ALL TESTS PASSED${NC}"
        exit 0
    fi
}

# Run the tests
main