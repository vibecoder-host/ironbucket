#!/bin/bash

# Test script for IronBucket CORS functionality
# This tests Cross-Origin Resource Sharing configuration

set +e

# Source test utilities
source "$(dirname "$0")/test-utils.sh"

# Load environment
load_test_env

# Check dependencies
check_dependencies

# Initialize test environment
echo "Testing IronBucket CORS"
echo "========================"

check_ironbucket_running

# Configure AWS CLI for testing
export AWS_ENDPOINT_URL=${S3_ENDPOINT}

# Create aws function to use endpoint URL consistently
aws() {
    command aws --endpoint-url ${S3_ENDPOINT} "$@"
}

# Test configuration
BUCKET="test-cors-$(date +%s)"

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

    # Delete test bucket
    aws s3 rb "s3://$BUCKET" --force 2>/dev/null || true

    # Remove temp files
    rm -f /tmp/cors-config*.json /tmp/cors-get*.json /tmp/cors-persist.json /tmp/cors-invalid.json
}

# Set up trap to cleanup on exit
trap cleanup EXIT

echo ""
echo "Starting tests..."
echo ""

# Test 1: Create test bucket
run_test "Create test bucket" "
    aws s3 mb s3://$BUCKET
"

# Test 2: Get CORS configuration (should be empty initially)
run_test "Get CORS configuration (empty)" "
    ! aws s3api get-bucket-cors --bucket $BUCKET 2>/dev/null
"

# Test 3: Set basic CORS configuration
run_test "Set basic CORS configuration" '
cat > /tmp/cors-config1.json <<EOF
{
  "CORSRules": [
    {
      "AllowedOrigins": ["*"],
      "AllowedMethods": ["GET", "PUT"],
      "AllowedHeaders": ["*"],
      "MaxAgeSeconds": 3000
    }
  ]
}
EOF
aws s3api put-bucket-cors --bucket $BUCKET --cors-configuration file:///tmp/cors-config1.json
'

# Test 4: Get CORS configuration
run_test "Get CORS configuration" "
    aws s3api get-bucket-cors --bucket $BUCKET > /tmp/cors-get1.json && \
    grep -q 'AllowedOrigin' /tmp/cors-get1.json && \
    grep -q 'AllowedMethod' /tmp/cors-get1.json
"

# Test 5: Set complex CORS configuration with multiple rules
run_test "Set complex CORS configuration" '
cat > /tmp/cors-config2.json <<EOF
{
  "CORSRules": [
    {
      "ID": "rule1",
      "AllowedOrigins": ["https://example.com", "https://app.example.com"],
      "AllowedMethods": ["GET", "POST", "PUT"],
      "AllowedHeaders": ["Authorization", "Content-Type"],
      "ExposeHeaders": ["ETag", "x-amz-request-id"],
      "MaxAgeSeconds": 3600
    },
    {
      "ID": "rule2",
      "AllowedOrigins": ["https://other.com"],
      "AllowedMethods": ["GET", "HEAD"],
      "MaxAgeSeconds": 1800
    }
  ]
}
EOF
aws s3api put-bucket-cors --bucket $BUCKET --cors-configuration file:///tmp/cors-config2.json
'

# Test 6: Verify complex CORS configuration
run_test "Verify complex CORS configuration" "
    aws s3api get-bucket-cors --bucket $BUCKET > /tmp/cors-get2.json && \
    grep -q 'rule1' /tmp/cors-get2.json && \
    grep -q 'rule2' /tmp/cors-get2.json && \
    grep -q 'https://example.com' /tmp/cors-get2.json && \
    grep -q 'Authorization' /tmp/cors-get2.json && \
    grep -q 'ETag' /tmp/cors-get2.json
"

# Test 7: Update CORS configuration
run_test "Update CORS configuration" '
cat > /tmp/cors-config3.json <<EOF
{
  "CORSRules": [
    {
      "AllowedOrigins": ["https://updated.com"],
      "AllowedMethods": ["GET", "DELETE"],
      "MaxAgeSeconds": 7200
    }
  ]
}
EOF
aws s3api put-bucket-cors --bucket $BUCKET --cors-configuration file:///tmp/cors-config3.json
'

# Test 8: Verify updated CORS configuration
run_test "Verify updated CORS configuration" "
    aws s3api get-bucket-cors --bucket $BUCKET > /tmp/cors-get3.json && \
    grep -q 'https://updated.com' /tmp/cors-get3.json && \
    grep -q 'DELETE' /tmp/cors-get3.json && \
    ! grep -q 'https://example.com' /tmp/cors-get3.json
"

# Test 9: Test CORS with wildcard origin
run_test "Set wildcard origin CORS" '
cat > /tmp/cors-config4.json <<EOF
{
  "CORSRules": [
    {
      "AllowedOrigins": ["*"],
      "AllowedMethods": ["GET", "PUT", "POST", "DELETE", "HEAD"],
      "AllowedHeaders": ["*"],
      "ExposeHeaders": ["*"],
      "MaxAgeSeconds": 86400
    }
  ]
}
EOF
aws s3api put-bucket-cors --bucket $BUCKET --cors-configuration file:///tmp/cors-config4.json
'

# Test 10: Verify wildcard CORS
run_test "Verify wildcard CORS" "
    aws s3api get-bucket-cors --bucket $BUCKET > /tmp/cors-get4.json && \
    grep -q '\"\\*\"' /tmp/cors-get4.json
"

# Test 11: Delete CORS configuration
run_test "Delete CORS configuration" "
    aws s3api delete-bucket-cors --bucket $BUCKET
"

# Test 12: Verify CORS is deleted
run_test "Verify CORS deleted" "
    ! aws s3api get-bucket-cors --bucket $BUCKET 2>/dev/null
"

# Test 13: Set CORS after deletion
run_test "Set CORS after deletion" '
cat > /tmp/cors-config5.json <<EOF
{
  "CORSRules": [
    {
      "AllowedOrigins": ["https://final.com"],
      "AllowedMethods": ["GET"],
      "MaxAgeSeconds": 300
    }
  ]
}
EOF
aws s3api put-bucket-cors --bucket $BUCKET --cors-configuration file:///tmp/cors-config5.json
'

# Test 14: CORS persistence
run_test "CORS configuration persistence" '
    # Set CORS
cat > /tmp/cors-persist.json <<EOF
{
  "CORSRules": [
    {
      "AllowedOrigins": ["https://persistent.com"],
      "AllowedMethods": ["GET", "PUT"]
    }
  ]
}
EOF
    aws s3api put-bucket-cors --bucket $BUCKET --cors-configuration file:///tmp/cors-persist.json && \
    sleep 2 && \
    # Verify it persists
    aws s3api get-bucket-cors --bucket $BUCKET | grep -q "persistent.com"
'

# Test 15: Invalid CORS configuration
run_test "Reject invalid CORS configuration" '
cat > /tmp/cors-invalid.json <<EOF
{
  "CORSRules": [
    {
      "MaxAgeSeconds": 300
    }
  ]
}
EOF
! aws s3api put-bucket-cors --bucket $BUCKET --cors-configuration file:///tmp/cors-invalid.json 2>/dev/null
'

# Test Summary
echo ""
echo "====================================="
echo "Test Summary:"
echo "  Total Tests: $TEST_COUNT"
echo -e "  Passed: ${GREEN}$PASS_COUNT${NC}"
echo -e "  Failed: ${RED}$FAIL_COUNT${NC}"

if [ $FAIL_COUNT -eq 0 ]; then
    echo -e "\n${GREEN}All CORS tests passed!${NC}"
    exit 0
else
    echo -e "\n${RED}Some CORS tests failed${NC}"
    exit 1
fi