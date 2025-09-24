#!/bin/bash

# Test utilities and common functions for IronBucket tests

# Colors for output
export GREEN='\033[0;32m'
export RED='\033[0;31m'
export YELLOW='\033[1;33m'
export NC='\033[0m' # No Color

# Load environment variables
load_test_env() {
    local env_file="${1:-$(dirname "$0")/.env}"

    if [ ! -f "$env_file" ]; then
        echo -e "${RED}Error: Environment file not found: $env_file${NC}"
        echo "Please copy .env.example to .env and configure it"
        exit 1
    fi

    # Export all variables from .env file
    set -a
    source "$env_file"
    set +a

    # Set AWS CLI environment variables
    export AWS_ACCESS_KEY_ID="${S3_ACCESS_KEY}"
    export AWS_SECRET_ACCESS_KEY="${S3_SECRET_KEY}"
    export AWS_DEFAULT_REGION="${S3_REGION}"

    echo -e "${GREEN}✓ Loaded configuration from $env_file${NC}"
}

# Check if required tools are installed
check_dependencies() {
    local missing_deps=()

    # Check for AWS CLI
    if ! command -v aws &> /dev/null; then
        missing_deps+=("awscli")
    fi

    # Check for jq
    if ! command -v jq &> /dev/null; then
        missing_deps+=("jq")
    fi

    # Check for curl
    if ! command -v curl &> /dev/null; then
        missing_deps+=("curl")
    fi

    if [ ${#missing_deps[@]} -gt 0 ]; then
        echo -e "${RED}Missing dependencies: ${missing_deps[*]}${NC}"
        echo "Install with: apt-get update && apt-get install -y ${missing_deps[*]}"
        exit 1
    fi
}

# Check if IronBucket is running
check_ironbucket_running() {
    if ! curl -s "${S3_ENDPOINT}/" 2>&1 | grep -q "Authentication required"; then
        echo -e "${RED}IronBucket is not running at ${S3_ENDPOINT}${NC}"
        echo "Start it with: cd /opt/app/ironbucket && docker compose up -d ironbucket"
        exit 1
    fi
    echo -e "${GREEN}✓ IronBucket is running at ${S3_ENDPOINT}${NC}"
}

# Create a test bucket
create_test_bucket() {
    local bucket="${1:-${TEST_BUCKET_PREFIX}-$(date +%s)}"

    if aws --endpoint-url="${S3_ENDPOINT}" s3 mb "s3://${bucket}" >/dev/null 2>&1; then
        echo -e "${GREEN}✓ Created bucket: ${bucket}${NC}" >&2
    else
        echo -e "${YELLOW}! Bucket may already exist: ${bucket}${NC}" >&2
    fi
    echo "${bucket}"
}

# Clean up test bucket
cleanup_test_bucket() {
    local bucket="$1"

    if [ -z "$bucket" ]; then
        echo -e "${RED}No bucket specified for cleanup${NC}"
        return 1
    fi

    # Remove all objects first
    aws --endpoint-url="${S3_ENDPOINT}" s3 rm "s3://${bucket}" --recursive 2>/dev/null || true

    # Remove the bucket
    if aws --endpoint-url="${S3_ENDPOINT}" s3 rb "s3://${bucket}" 2>/dev/null; then
        echo -e "${GREEN}✓ Cleaned up bucket: ${bucket}${NC}"
    else
        echo -e "${YELLOW}! Could not remove bucket: ${bucket}${NC}"
    fi

    # Also clean up from filesystem
    if [ -d "${STORAGE_PATH}/${bucket}" ]; then
        rm -rf "${STORAGE_PATH}/${bucket}"
        echo -e "${GREEN}✓ Cleaned up filesystem: ${STORAGE_PATH}/${bucket}${NC}"
    fi
}

# Upload a test file
upload_test_file() {
    local bucket="$1"
    local key="$2"
    local content="$3"
    local content_type="${4:-text/plain}"

    local temp_file="/tmp/test-upload-$$"
    echo "$content" > "$temp_file"

    local error_output
    if error_output=$(aws --endpoint-url="${S3_ENDPOINT}" s3 cp "$temp_file" "s3://${bucket}/${key}" \
        --content-type "$content_type" 2>&1); then
        echo -e "${GREEN}✓ Uploaded: ${key}${NC}" >&2
        rm -f "$temp_file"
        return 0
    else
        echo -e "${RED}✗ Failed to upload: ${key}${NC}" >&2
        echo -e "${RED}  Error: ${error_output}${NC}" >&2
        rm -f "$temp_file"
        return 1
    fi
}

# Check if metadata file exists
check_metadata_exists() {
    local bucket="$1"
    local key="$2"

    # Metadata files are stored as key.metadata (including extension)
    local metadata_file="${STORAGE_PATH}/${bucket}/${key}.metadata"

    if [ -f "$metadata_file" ]; then
        echo -e "${GREEN}✓ Metadata exists: ${metadata_file}${NC}"
        return 0
    else
        echo -e "${RED}✗ Metadata missing: ${metadata_file}${NC}"
        return 1
    fi
}

# Get metadata content
get_metadata_content() {
    local bucket="$1"
    local key="$2"

    # Metadata files are stored as key.metadata (including extension)
    local metadata_file="${STORAGE_PATH}/${bucket}/${key}.metadata"

    if [ -f "$metadata_file" ]; then
        cat "$metadata_file"
    else
        echo "{}"
    fi
}

# Run a test with proper formatting
run_test() {
    local test_name="$1"
    shift

    echo -e "\n${YELLOW}▶ ${test_name}${NC}"
    "$@"
    local result=$?

    if [ $result -eq 0 ]; then
        echo -e "${GREEN}✓ ${test_name} passed${NC}"
    else
        echo -e "${RED}✗ ${test_name} failed${NC}"
    fi

    return $result
}

# Print test summary
print_summary() {
    local passed=$1
    local failed=$2
    local total=$((passed + failed))

    echo -e "\n${YELLOW}═══════════════════════════════════════${NC}"
    echo -e "${YELLOW}Test Summary${NC}"
    echo -e "${YELLOW}═══════════════════════════════════════${NC}"

    if [ $failed -eq 0 ]; then
        echo -e "${GREEN}✓ All tests passed! (${passed}/${total})${NC}"
    else
        echo -e "${RED}✗ Some tests failed!${NC}"
        echo -e "  ${GREEN}Passed: ${passed}${NC}"
        echo -e "  ${RED}Failed: ${failed}${NC}"
    fi

    echo -e "${YELLOW}═══════════════════════════════════════${NC}"

    return $failed
}