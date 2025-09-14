#!/bin/bash

# Load test utilities
source "$(dirname "$0")/test-utils.sh"

# Test metadata persistence functionality
test_metadata_persistence() {
    echo "Testing IronBucket Metadata Persistence"
    echo "========================================"

    # Load environment and check dependencies
    load_test_env
    check_dependencies
    check_ironbucket_running

    # Create a unique test bucket
    BUCKET=$(create_test_bucket "${TEST_BUCKET_PREFIX}-metadata")

    local passed=0
    local failed=0

    # Test 1: Upload file with specific content-type
    if run_test "Upload with custom content-type" test_upload_with_content_type "$BUCKET"; then
        ((passed++))
    else
        ((failed++))
    fi

    # Test 2: Upload JSON file
    if run_test "Upload JSON file with metadata" test_json_upload "$BUCKET"; then
        ((passed++))
    else
        ((failed++))
    fi

    # Test 3: HEAD request returns metadata
    if run_test "HEAD request returns metadata" test_head_request "$BUCKET"; then
        ((passed++))
    else
        ((failed++))
    fi

    # Test 4: GET request works with metadata
    if run_test "Download file and verify" test_download_file "$BUCKET"; then
        ((passed++))
    else
        ((failed++))
    fi

    # Test 5: List objects
    if run_test "List objects in bucket" test_list_objects "$BUCKET"; then
        ((passed++))
    else
        ((failed++))
    fi

    # Test 6: Metadata persistence after restart
    if run_test "Metadata persistence after restart" test_persistence_after_restart "$BUCKET"; then
        ((passed++))
    else
        ((failed++))
    fi

    # Cleanup
    cleanup_test_bucket "$BUCKET"

    # Print summary
    print_summary $passed $failed
}

test_upload_with_content_type() {
    local bucket="$1"

    # Upload a text file
    upload_test_file "$bucket" "test-file.txt" "This is a test file" "text/plain"

    # Check if metadata file was created
    if ! check_metadata_exists "$bucket" "test-file.txt"; then
        return 1
    fi

    # Verify content-type in metadata
    local content_type=$(get_metadata_content "$bucket" "test-file.txt" | jq -r .content_type)
    if [ "$content_type" = "text/plain" ]; then
        echo -e "  ${GREEN}Content-type correctly stored: $content_type${NC}"
        return 0
    else
        echo -e "  ${RED}Wrong content-type: $content_type${NC}"
        return 1
    fi
}

test_json_upload() {
    local bucket="$1"

    # Upload JSON file
    upload_test_file "$bucket" "test.json" '{"test": "data"}' "application/json"

    # Check metadata exists
    if ! check_metadata_exists "$bucket" "test.json"; then
        return 1
    fi

    # Verify content-type
    local content_type=$(get_metadata_content "$bucket" "test.json" | jq -r .content_type)
    if [ "$content_type" = "application/json" ]; then
        echo -e "  ${GREEN}JSON content-type correct: $content_type${NC}"
        return 0
    else
        echo -e "  ${RED}Wrong JSON content-type: $content_type${NC}"
        return 1
    fi
}

test_head_request() {
    local bucket="$1"

    # Make HEAD request
    local head_response=$(aws --endpoint-url="${S3_ENDPOINT}" s3api head-object \
        --bucket "$bucket" --key "test.json" 2>&1)

    if echo "$head_response" | grep -q "application/json"; then
        echo -e "  ${GREEN}HEAD request returns correct content-type${NC}"
        return 0
    else
        echo -e "  ${RED}HEAD request doesn't return correct content-type${NC}"
        return 1
    fi
}

test_download_file() {
    local bucket="$1"
    local temp_download="/tmp/test-download-$$"

    # Download the file
    if aws --endpoint-url="${S3_ENDPOINT}" s3 cp "s3://${bucket}/test.json" "$temp_download" >/dev/null 2>&1; then
        # Verify content
        if grep -q '"test"' "$temp_download"; then
            echo -e "  ${GREEN}File downloaded and verified successfully${NC}"
            rm -f "$temp_download"
            return 0
        else
            echo -e "  ${RED}Downloaded file has wrong content${NC}"
            rm -f "$temp_download"
            return 1
        fi
    else
        echo -e "  ${RED}Download failed${NC}"
        return 1
    fi
}

test_list_objects() {
    local bucket="$1"

    # List objects
    local objects=$(aws --endpoint-url="${S3_ENDPOINT}" s3 ls "s3://${bucket}/" 2>&1)

    if echo "$objects" | grep -q "test.json" && echo "$objects" | grep -q "test-file.txt"; then
        echo -e "  ${GREEN}Both test files listed correctly${NC}"
        return 0
    else
        echo -e "  ${RED}Files not listed correctly${NC}"
        echo "$objects"
        return 1
    fi
}

test_persistence_after_restart() {
    local bucket="$1"

    echo "  Restarting IronBucket..."
    cd /opt/app/ironbucket && docker compose restart ironbucket >/dev/null 2>&1
    sleep 5

    # Check if metadata still exists and is readable
    local head_response=$(aws --endpoint-url="${S3_ENDPOINT}" s3api head-object \
        --bucket "$bucket" --key "test.json" 2>&1)

    if echo "$head_response" | grep -q "application/json"; then
        echo -e "  ${GREEN}Metadata persisted after restart${NC}"
        return 0
    else
        echo -e "  ${RED}Metadata lost after restart${NC}"
        return 1
    fi
}

# Run the tests
test_metadata_persistence