#!/bin/bash

# Test script for IronBucket encryption module functionality
# This tests the ring-based encryption implementation against the running Docker container

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
PORT=20000  # Using the running container port
S3_ENDPOINT="http://172.17.0.1:$PORT"
ACCESS_KEY="${ACCESS_KEY:-root}"
SECRET_KEY="${SECRET_KEY:-xxxxxxxxxxxxxxx}"

# Test counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# Cleanup function
cleanup() {
    echo -e "\n${BLUE}Cleaning up...${NC}"

    # Clean up test directory
    rm -rf "$TEST_DIR"

    # Clean up AWS CLI config
    rm -f ~/.aws/credentials.test
    rm -f ~/.aws/config.test

    # Clean up test buckets
    aws --endpoint-url=$S3_ENDPOINT s3 rb s3://$BUCKET_NAME --force 2>/dev/null || true
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

    # Test connectivity to the running container
    echo -e "${BLUE}Testing connection to IronBucket at $S3_ENDPOINT...${NC}"
    if aws --endpoint-url=$S3_ENDPOINT s3 ls >/dev/null 2>&1; then
        echo -e "${GREEN}✓ Successfully connected to IronBucket${NC}"
    else
        echo -e "${RED}✗ Failed to connect to IronBucket at $S3_ENDPOINT${NC}"
        echo -e "${YELLOW}Make sure IronBucket is running in Docker${NC}"
        exit 1
    fi
}

# Main test execution
main() {
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}IronBucket Encryption Module Tests${NC}"
    echo -e "${BLUE}(Testing against running Docker container)${NC}"
    echo -e "${BLUE}========================================${NC}"

    setup_test_env

    # Test 1: Basic operations with current server configuration
    echo -e "\n${YELLOW}Test Suite 1: Basic encryption operations${NC}"

    # Create bucket
    aws --endpoint-url=$S3_ENDPOINT s3 mb s3://$BUCKET_NAME 2>/dev/null
    assert_success $? "Create bucket"

    # Upload object
    echo "Test data for encryption" > $TEST_DIR/test-file.txt
    aws --endpoint-url=$S3_ENDPOINT s3 cp $TEST_DIR/test-file.txt s3://$BUCKET_NAME/$OBJECT_KEY 2>/dev/null
    assert_success $? "Upload object"

    # Download and verify
    aws --endpoint-url=$S3_ENDPOINT s3 cp s3://$BUCKET_NAME/$OBJECT_KEY $TEST_DIR/downloaded.txt 2>/dev/null
    assert_success $? "Download object"

    content=$(cat $TEST_DIR/downloaded.txt)
    assert_equals "Test data for encryption" "$content" "Content matches after round-trip"

    # Cleanup
    aws --endpoint-url=$S3_ENDPOINT s3 rm s3://$BUCKET_NAME/$OBJECT_KEY 2>/dev/null
    rm -f $TEST_DIR/test-file.txt $TEST_DIR/downloaded.txt

    # Test 2: Bucket-level encryption configuration
    echo -e "\n${YELLOW}Test Suite 2: Bucket-level encryption configuration${NC}"

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
    aws --endpoint-url=$S3_ENDPOINT s3 cp $TEST_DIR/test-file.txt s3://$BUCKET_NAME/encrypted-object 2>/dev/null
    assert_success $? "Upload object to encrypted bucket"

    # Download and verify
    aws --endpoint-url=$S3_ENDPOINT s3 cp s3://$BUCKET_NAME/encrypted-object $TEST_DIR/downloaded.txt 2>/dev/null
    assert_success $? "Download object from encrypted bucket"

    content=$(cat $TEST_DIR/downloaded.txt)
    assert_equals "Test data in encrypted bucket" "$content" "Content matches from encrypted bucket"

    # Test 3: Mixed encrypted and unencrypted objects
    echo -e "\n${YELLOW}Test Suite 3: Mixed encrypted and unencrypted objects${NC}"

    # Upload unencrypted object first (before encryption was enabled)
    echo "Object before encryption" > $TEST_DIR/before.txt

    # Delete bucket encryption temporarily
    aws --endpoint-url=$S3_ENDPOINT s3api delete-bucket-encryption \
        --bucket $BUCKET_NAME 2>/dev/null
    assert_success $? "Delete bucket encryption temporarily"

    # Upload unencrypted object
    aws --endpoint-url=$S3_ENDPOINT s3 cp $TEST_DIR/before.txt s3://$BUCKET_NAME/unencrypted 2>/dev/null
    assert_success $? "Upload unencrypted object"

    # Re-enable bucket encryption
    aws --endpoint-url=$S3_ENDPOINT s3api put-bucket-encryption \
        --bucket $BUCKET_NAME \
        --server-side-encryption-configuration file://$TEST_DIR/encryption-config.json 2>/dev/null
    assert_success $? "Re-enable bucket encryption"

    # Upload encrypted object
    echo "Object after encryption" > $TEST_DIR/after.txt
    aws --endpoint-url=$S3_ENDPOINT s3 cp $TEST_DIR/after.txt s3://$BUCKET_NAME/encrypted 2>/dev/null
    assert_success $? "Upload encrypted object after enabling encryption"

    # Download both objects
    aws --endpoint-url=$S3_ENDPOINT s3 cp s3://$BUCKET_NAME/unencrypted $TEST_DIR/downloaded-unencrypted.txt 2>/dev/null
    assert_success $? "Download unencrypted object from mixed bucket"

    aws --endpoint-url=$S3_ENDPOINT s3 cp s3://$BUCKET_NAME/encrypted $TEST_DIR/downloaded-encrypted.txt 2>/dev/null
    assert_success $? "Download encrypted object from mixed bucket"

    # Verify content
    unencrypted_content=$(cat $TEST_DIR/downloaded-unencrypted.txt)
    assert_equals "Object before encryption" "$unencrypted_content" "Unencrypted object content matches"

    encrypted_content=$(cat $TEST_DIR/downloaded-encrypted.txt)
    assert_equals "Object after encryption" "$encrypted_content" "Encrypted object content matches"

    # Test 4: Large file encryption
    echo -e "\n${YELLOW}Test Suite 4: Large file encryption${NC}"

    # Create a 5MB file (smaller for Docker testing)
    dd if=/dev/urandom of=$TEST_DIR/large-file.bin bs=1M count=5 2>/dev/null
    assert_success $? "Create 5MB test file"

    # Calculate checksum of original
    original_checksum=$(md5sum $TEST_DIR/large-file.bin | cut -d' ' -f1)

    # Upload large file to encrypted bucket
    aws --endpoint-url=$S3_ENDPOINT s3 cp $TEST_DIR/large-file.bin s3://$BUCKET_NAME/large-object 2>/dev/null
    assert_success $? "Upload 5MB file to encrypted bucket"

    # Download and verify checksum
    aws --endpoint-url=$S3_ENDPOINT s3 cp s3://$BUCKET_NAME/large-object $TEST_DIR/downloaded-large.bin 2>/dev/null
    assert_success $? "Download 5MB encrypted file"

    downloaded_checksum=$(md5sum $TEST_DIR/downloaded-large.bin | cut -d' ' -f1)
    assert_equals "$original_checksum" "$downloaded_checksum" "Large file checksum matches after encryption/decryption"

    # Test 5: Delete encryption configuration
    echo -e "\n${YELLOW}Test Suite 5: Delete encryption configuration${NC}"

    # Delete bucket encryption
    aws --endpoint-url=$S3_ENDPOINT s3api delete-bucket-encryption \
        --bucket $BUCKET_NAME 2>/dev/null
    assert_success $? "Delete bucket encryption configuration"

    # Verify deletion
    aws --endpoint-url=$S3_ENDPOINT s3api get-bucket-encryption \
        --bucket $BUCKET_NAME > $TEST_DIR/no-encryption.json 2>&1
    assert_failure $? "Get bucket encryption after deletion should fail"

    # Upload new object after disabling encryption
    echo "Object without encryption" > $TEST_DIR/no-enc.txt
    aws --endpoint-url=$S3_ENDPOINT s3 cp $TEST_DIR/no-enc.txt s3://$BUCKET_NAME/no-encryption 2>/dev/null
    assert_success $? "Upload object after disabling encryption"

    # Download and verify
    aws --endpoint-url=$S3_ENDPOINT s3 cp s3://$BUCKET_NAME/no-encryption $TEST_DIR/downloaded-no-enc.txt 2>/dev/null
    assert_success $? "Download object uploaded after disabling encryption"

    no_enc_content=$(cat $TEST_DIR/downloaded-no-enc.txt)
    assert_equals "Object without encryption" "$no_enc_content" "Object uploaded without encryption matches"

    # Cleanup bucket
    aws --endpoint-url=$S3_ENDPOINT s3 rb s3://$BUCKET_NAME --force 2>/dev/null
    assert_success $? "Delete test bucket"

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