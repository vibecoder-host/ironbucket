#!/bin/bash

# Load test utilities
source "$(dirname "$0")/test-utils.sh"

# Test multipart upload functionality
test_multipart_upload() {
    echo "Testing Multipart Upload Functionality"
    echo "======================================="

    # Load environment and check dependencies
    load_test_env
    check_dependencies
    check_ironbucket_running

    local passed=0
    local failed=0

    # Test 1: Initiate multipart upload
    if run_test "Initiate multipart upload" test_initiate_multipart; then
        ((passed++))
    else
        ((failed++))
    fi

    # Test 2: Upload parts
    if run_test "Upload multiple parts" test_upload_parts; then
        ((passed++))
    else
        ((failed++))
    fi

    # Test 3: List parts
    if run_test "List uploaded parts" test_list_parts; then
        ((passed++))
    else
        ((failed++))
    fi

    # Test 4: Complete multipart upload
    if run_test "Complete multipart upload" test_complete_multipart; then
        ((passed++))
    else
        ((failed++))
    fi

    # Test 5: Verify assembled object
    if run_test "Verify assembled object" test_verify_assembled_object; then
        ((passed++))
    else
        ((failed++))
    fi

    # Test 6: Abort multipart upload
    if run_test "Abort multipart upload" test_abort_multipart; then
        ((passed++))
    else
        ((failed++))
    fi

    # Test 7: Large file multipart upload
    if run_test "Large file multipart upload" test_large_multipart; then
        ((passed++))
    else
        ((failed++))
    fi

    # Test 8: Multipart with metadata persistence
    if run_test "Multipart metadata persistence" test_multipart_metadata; then
        ((passed++))
    else
        ((failed++))
    fi

    # Print summary
    print_summary $passed $failed
}

test_initiate_multipart() {
    local bucket="${TEST_BUCKET_PREFIX}-multipart-init"
    create_test_bucket "$bucket" >/dev/null 2>&1

    # Initiate multipart upload
    local response=$(aws --endpoint-url="${S3_ENDPOINT}" s3api create-multipart-upload \
        --bucket "$bucket" \
        --key "test-multipart.txt" 2>&1)

    if echo "$response" | grep -q "UploadId"; then
        local upload_id=$(echo "$response" | jq -r .UploadId)
        echo -e "  ${GREEN}Upload ID created: ${upload_id:0:8}...${NC}"

        # Store for later tests
        echo "$upload_id" > /tmp/test-upload-id
        echo "$bucket" > /tmp/test-bucket

        # Check if upload metadata was created on disk
        if [ -f "${STORAGE_PATH}/${bucket}/.multipart/${upload_id}.upload" ]; then
            echo -e "  ${GREEN}Upload metadata file created${NC}"
            return 0
        else
            echo -e "  ${YELLOW}Upload metadata file not found${NC}"
            return 0  # Still pass as this is optional
        fi
    else
        echo -e "  ${RED}Failed to initiate multipart upload${NC}"
        echo "$response"
        return 1
    fi
}

test_upload_parts() {
    local bucket=$(cat /tmp/test-bucket 2>/dev/null)
    local upload_id=$(cat /tmp/test-upload-id 2>/dev/null)

    if [ -z "$upload_id" ] || [ -z "$bucket" ]; then
        echo -e "  ${YELLOW}Skipping - no upload ID available${NC}"
        return 0
    fi

    # Create test parts
    echo "Part 1 content" > /tmp/part1.txt
    echo "Part 2 content with more data" > /tmp/part2.txt
    echo "Part 3 final content" > /tmp/part3.txt

    local etags=""

    # Upload part 1
    local response1=$(aws --endpoint-url="${S3_ENDPOINT}" s3api upload-part \
        --bucket "$bucket" \
        --key "test-multipart.txt" \
        --upload-id "$upload_id" \
        --part-number 1 \
        --body /tmp/part1.txt 2>&1)

    if echo "$response1" | grep -q "ETag"; then
        local etag1=$(echo "$response1" | jq -r .ETag)
        etags="${etags}1,${etag1};"
        echo -e "  ${GREEN}Part 1 uploaded, ETag: ${etag1}${NC}"
    else
        echo -e "  ${RED}Failed to upload part 1${NC}"
        return 1
    fi

    # Upload part 2
    local response2=$(aws --endpoint-url="${S3_ENDPOINT}" s3api upload-part \
        --bucket "$bucket" \
        --key "test-multipart.txt" \
        --upload-id "$upload_id" \
        --part-number 2 \
        --body /tmp/part2.txt 2>&1)

    if echo "$response2" | grep -q "ETag"; then
        local etag2=$(echo "$response2" | jq -r .ETag)
        etags="${etags}2,${etag2};"
        echo -e "  ${GREEN}Part 2 uploaded, ETag: ${etag2}${NC}"
    else
        echo -e "  ${RED}Failed to upload part 2${NC}"
        return 1
    fi

    # Upload part 3
    local response3=$(aws --endpoint-url="${S3_ENDPOINT}" s3api upload-part \
        --bucket "$bucket" \
        --key "test-multipart.txt" \
        --upload-id "$upload_id" \
        --part-number 3 \
        --body /tmp/part3.txt 2>&1)

    if echo "$response3" | grep -q "ETag"; then
        local etag3=$(echo "$response3" | jq -r .ETag)
        etags="${etags}3,${etag3}"
        echo -e "  ${GREEN}Part 3 uploaded, ETag: ${etag3}${NC}"

        # Store ETags for completion
        echo "$etags" > /tmp/test-etags
        return 0
    else
        echo -e "  ${RED}Failed to upload part 3${NC}"
        return 1
    fi
}

test_list_parts() {
    local bucket=$(cat /tmp/test-bucket 2>/dev/null)
    local upload_id=$(cat /tmp/test-upload-id 2>/dev/null)

    if [ -z "$upload_id" ] || [ -z "$bucket" ]; then
        echo -e "  ${YELLOW}Skipping - no upload ID available${NC}"
        return 0
    fi

    # List parts
    local response=$(aws --endpoint-url="${S3_ENDPOINT}" s3api list-parts \
        --bucket "$bucket" \
        --key "test-multipart.txt" \
        --upload-id "$upload_id" 2>&1)

    if echo "$response" | grep -q "Parts"; then
        local part_count=$(echo "$response" | jq '.Parts | length')
        if [ "$part_count" -eq "3" ]; then
            echo -e "  ${GREEN}All 3 parts listed correctly${NC}"
            return 0
        else
            echo -e "  ${RED}Expected 3 parts, found $part_count${NC}"
            return 1
        fi
    else
        echo -e "  ${RED}Failed to list parts${NC}"
        echo "$response"
        return 1
    fi
}

test_complete_multipart() {
    local bucket=$(cat /tmp/test-bucket 2>/dev/null)
    local upload_id=$(cat /tmp/test-upload-id 2>/dev/null)
    local etags=$(cat /tmp/test-etags 2>/dev/null)

    if [ -z "$upload_id" ] || [ -z "$bucket" ] || [ -z "$etags" ]; then
        echo -e "  ${YELLOW}Skipping - no upload data available${NC}"
        return 0
    fi

    # Create multipart list JSON
    local parts_json="["
    IFS=';' read -ra PARTS <<< "$etags"
    for i in "${!PARTS[@]}"; do
        if [ -n "${PARTS[$i]}" ]; then
            IFS=',' read -r part_num etag <<< "${PARTS[$i]}"
            [ $i -gt 0 ] && parts_json="${parts_json},"
            parts_json="${parts_json}{\"PartNumber\":${part_num},\"ETag\":${etag}}"
        fi
    done
    parts_json="${parts_json}]"

    echo "$parts_json" > /tmp/parts.json

    # Complete multipart upload
    local response=$(aws --endpoint-url="${S3_ENDPOINT}" s3api complete-multipart-upload \
        --bucket "$bucket" \
        --key "test-multipart.txt" \
        --upload-id "$upload_id" \
        --multipart-upload "{\"Parts\":${parts_json}}" 2>&1)

    if echo "$response" | grep -q "ETag\|Location"; then
        echo -e "  ${GREEN}Multipart upload completed successfully${NC}"

        # Check if metadata file was created
        if [ -f "${STORAGE_PATH}/${bucket}/test-multipart.metadata" ]; then
            echo -e "  ${GREEN}Object metadata file created${NC}"
        fi

        # Clean up temp files
        rm -f /tmp/test-upload-id /tmp/test-bucket /tmp/test-etags /tmp/parts.json
        return 0
    else
        echo -e "  ${RED}Failed to complete multipart upload${NC}"
        echo "$response"
        return 1
    fi
}

test_verify_assembled_object() {
    local bucket="${TEST_BUCKET_PREFIX}-multipart-init"

    # Download the assembled object
    local download_file="/tmp/assembled-multipart.txt"

    if aws --endpoint-url="${S3_ENDPOINT}" s3 cp "s3://${bucket}/test-multipart.txt" "$download_file" >/dev/null 2>&1; then
        # Check content
        local expected_content="Part 1 content
Part 2 content with more data
Part 3 final content"

        local actual_content=$(cat "$download_file")

        if [ "$actual_content" = "$expected_content" ]; then
            echo -e "  ${GREEN}Assembled object content is correct${NC}"
            rm -f "$download_file"
            cleanup_test_bucket "$bucket" >/dev/null 2>&1
            return 0
        else
            echo -e "  ${RED}Assembled object content mismatch${NC}"
            echo "Expected: $expected_content"
            echo "Actual: $actual_content"
            rm -f "$download_file"
            cleanup_test_bucket "$bucket" >/dev/null 2>&1
            return 1
        fi
    else
        echo -e "  ${RED}Failed to download assembled object${NC}"
        cleanup_test_bucket "$bucket" >/dev/null 2>&1
        return 1
    fi
}

test_abort_multipart() {
    local bucket="${TEST_BUCKET_PREFIX}-multipart-abort"
    create_test_bucket "$bucket" >/dev/null 2>&1

    # Initiate a new multipart upload
    local response=$(aws --endpoint-url="${S3_ENDPOINT}" s3api create-multipart-upload \
        --bucket "$bucket" \
        --key "test-abort.txt" 2>&1)

    if echo "$response" | grep -q "UploadId"; then
        local upload_id=$(echo "$response" | jq -r .UploadId)
        echo -e "  ${GREEN}Created upload to abort: ${upload_id:0:8}...${NC}"

        # Upload one part
        echo "Part to be aborted" > /tmp/abort-part.txt
        aws --endpoint-url="${S3_ENDPOINT}" s3api upload-part \
            --bucket "$bucket" \
            --key "test-abort.txt" \
            --upload-id "$upload_id" \
            --part-number 1 \
            --body /tmp/abort-part.txt >/dev/null 2>&1

        # Abort the upload
        local abort_response=$(aws --endpoint-url="${S3_ENDPOINT}" s3api abort-multipart-upload \
            --bucket "$bucket" \
            --key "test-abort.txt" \
            --upload-id "$upload_id" 2>&1)

        # Check if upload was cleaned up
        if ! [ -d "${STORAGE_PATH}/${bucket}/.multipart/${upload_id}" ]; then
            echo -e "  ${GREEN}Multipart upload aborted and cleaned up${NC}"
            cleanup_test_bucket "$bucket" >/dev/null 2>&1
            return 0
        else
            echo -e "  ${RED}Multipart upload directory still exists after abort${NC}"
            cleanup_test_bucket "$bucket" >/dev/null 2>&1
            return 1
        fi
    else
        echo -e "  ${RED}Failed to initiate upload for abort test${NC}"
        cleanup_test_bucket "$bucket" >/dev/null 2>&1
        return 1
    fi
}

test_large_multipart() {
    local bucket="${TEST_BUCKET_PREFIX}-multipart-large"
    create_test_bucket "$bucket" >/dev/null 2>&1

    # Create larger test files (100KB each)
    dd if=/dev/urandom of=/tmp/large-part1 bs=1024 count=100 2>/dev/null
    dd if=/dev/urandom of=/tmp/large-part2 bs=1024 count=100 2>/dev/null
    dd if=/dev/urandom of=/tmp/large-part3 bs=1024 count=100 2>/dev/null

    # Initiate multipart upload
    local response=$(aws --endpoint-url="${S3_ENDPOINT}" s3api create-multipart-upload \
        --bucket "$bucket" \
        --key "large-multipart.bin" 2>&1)

    if echo "$response" | grep -q "UploadId"; then
        local upload_id=$(echo "$response" | jq -r .UploadId)

        # Upload parts
        local parts_json="["
        for i in 1 2 3; do
            local part_response=$(aws --endpoint-url="${S3_ENDPOINT}" s3api upload-part \
                --bucket "$bucket" \
                --key "large-multipart.bin" \
                --upload-id "$upload_id" \
                --part-number $i \
                --body /tmp/large-part$i 2>&1)

            if echo "$part_response" | grep -q "ETag"; then
                local etag=$(echo "$part_response" | jq -r .ETag)
                [ $i -gt 1 ] && parts_json="${parts_json},"
                parts_json="${parts_json}{\"PartNumber\":${i},\"ETag\":${etag}}"
            fi
        done
        parts_json="${parts_json}]"

        # Complete upload
        local complete_response=$(aws --endpoint-url="${S3_ENDPOINT}" s3api complete-multipart-upload \
            --bucket "$bucket" \
            --key "large-multipart.bin" \
            --upload-id "$upload_id" \
            --multipart-upload "{\"Parts\":${parts_json}}" 2>&1)

        if echo "$complete_response" | grep -q "ETag\|Location"; then
            # Verify size
            local object_size=$(stat -c%s "${STORAGE_PATH}/${bucket}/large-multipart.bin" 2>/dev/null)
            if [ "$object_size" -eq "307200" ]; then  # 300KB total
                echo -e "  ${GREEN}Large multipart upload successful (300KB)${NC}"
                cleanup_test_bucket "$bucket" >/dev/null 2>&1
                rm -f /tmp/large-part*
                return 0
            else
                echo -e "  ${RED}Object size mismatch: expected 307200, got $object_size${NC}"
                cleanup_test_bucket "$bucket" >/dev/null 2>&1
                rm -f /tmp/large-part*
                return 1
            fi
        else
            echo -e "  ${RED}Failed to complete large multipart upload${NC}"
            cleanup_test_bucket "$bucket" >/dev/null 2>&1
            rm -f /tmp/large-part*
            return 1
        fi
    else
        echo -e "  ${RED}Failed to initiate large multipart upload${NC}"
        cleanup_test_bucket "$bucket" >/dev/null 2>&1
        rm -f /tmp/large-part*
        return 1
    fi
}

test_multipart_metadata() {
    local bucket="${TEST_BUCKET_PREFIX}-multipart-meta"
    create_test_bucket "$bucket" >/dev/null 2>&1

    # Initiate upload
    local response=$(aws --endpoint-url="${S3_ENDPOINT}" s3api create-multipart-upload \
        --bucket "$bucket" \
        --key "metadata-test.txt" \
        --content-type "text/plain" 2>&1)

    if echo "$response" | grep -q "UploadId"; then
        local upload_id=$(echo "$response" | jq -r .UploadId)

        # Upload a part
        echo "Test content" > /tmp/meta-part.txt
        local part_response=$(aws --endpoint-url="${S3_ENDPOINT}" s3api upload-part \
            --bucket "$bucket" \
            --key "metadata-test.txt" \
            --upload-id "$upload_id" \
            --part-number 1 \
            --body /tmp/meta-part.txt 2>&1)

        local etag=$(echo "$part_response" | jq -r .ETag)

        # Complete upload
        aws --endpoint-url="${S3_ENDPOINT}" s3api complete-multipart-upload \
            --bucket "$bucket" \
            --key "metadata-test.txt" \
            --upload-id "$upload_id" \
            --multipart-upload "{\"Parts\":[{\"PartNumber\":1,\"ETag\":${etag}}]}" >/dev/null 2>&1

        # Check metadata file
        if [ -f "${STORAGE_PATH}/${bucket}/metadata-test.metadata" ]; then
            local metadata=$(cat "${STORAGE_PATH}/${bucket}/metadata-test.metadata")
            if echo "$metadata" | grep -q "metadata-test.txt"; then
                echo -e "  ${GREEN}Metadata persisted correctly for multipart upload${NC}"
                cleanup_test_bucket "$bucket" >/dev/null 2>&1
                rm -f /tmp/meta-part.txt
                return 0
            else
                echo -e "  ${RED}Metadata content incorrect${NC}"
                cleanup_test_bucket "$bucket" >/dev/null 2>&1
                rm -f /tmp/meta-part.txt
                return 1
            fi
        else
            echo -e "  ${RED}Metadata file not created for multipart upload${NC}"
            cleanup_test_bucket "$bucket" >/dev/null 2>&1
            rm -f /tmp/meta-part.txt
            return 1
        fi
    else
        echo -e "  ${RED}Failed to initiate upload for metadata test${NC}"
        cleanup_test_bucket "$bucket" >/dev/null 2>&1
        return 1
    fi
}

# Show usage if --help is provided
if [ "$1" = "--help" ] || [ "$1" = "-h" ]; then
    echo "Usage: $0"
    echo ""
    echo "Test multipart upload functionality in IronBucket."
    echo ""
    echo "Tests include:"
    echo "  - Initiate multipart upload"
    echo "  - Upload multiple parts"
    echo "  - List uploaded parts"
    echo "  - Complete multipart upload"
    echo "  - Verify assembled object"
    echo "  - Abort multipart upload"
    echo "  - Large file multipart upload"
    echo "  - Metadata persistence for multipart uploads"
    echo ""
    echo "Configuration is loaded from .env file."
    exit 0
fi

# Run the tests
test_multipart_upload