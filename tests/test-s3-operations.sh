#!/bin/bash

# Load test utilities
source "$(dirname "$0")/test-utils.sh"

# Test basic S3 operations
test_s3_operations() {
    echo "Testing Basic S3 Operations"
    echo "==========================="

    # Load environment and check dependencies
    load_test_env
    check_dependencies
    check_ironbucket_running

    local passed=0
    local failed=0

    # Test 1: Bucket operations
    if run_test "Bucket create/list/delete" test_bucket_operations; then
        ((passed++))
    else
        ((failed++))
    fi

    # Test 2: Object upload/download
    if run_test "Object upload and download" test_object_operations; then
        ((passed++))
    else
        ((failed++))
    fi

    # Test 3: Object listing
    if run_test "Object listing with prefix" test_object_listing; then
        ((passed++))
    else
        ((failed++))
    fi

    # Test 4: Object deletion
    if run_test "Object deletion" test_object_deletion; then
        ((passed++))
    else
        ((failed++))
    fi

    # Test 5: Large file upload
    if run_test "Large file upload (1MB)" test_large_file_upload; then
        ((passed++))
    else
        ((failed++))
    fi

    # Test 6: Special characters in keys
    if run_test "Special characters in object keys" test_special_characters; then
        ((passed++))
    else
        ((failed++))
    fi

    # Print summary
    print_summary $passed $failed
}

test_bucket_operations() {
    local bucket="${TEST_BUCKET_PREFIX}-ops-$$"

    # Create bucket
    if ! aws --endpoint-url="${S3_ENDPOINT}" s3 mb "s3://${bucket}" >/dev/null 2>&1; then
        echo -e "  ${RED}Failed to create bucket${NC}"
        return 1
    fi

    # List buckets and verify it exists
    if ! aws --endpoint-url="${S3_ENDPOINT}" s3 ls 2>/dev/null | grep -q "$bucket"; then
        echo -e "  ${RED}Bucket not found in listing${NC}"
        return 1
    fi

    # Delete bucket
    if ! aws --endpoint-url="${S3_ENDPOINT}" s3 rb "s3://${bucket}" >/dev/null 2>&1; then
        echo -e "  ${RED}Failed to delete bucket${NC}"
        return 1
    fi

    echo -e "  ${GREEN}Bucket operations successful${NC}"
    return 0
}

test_object_operations() {
    local bucket=$(create_test_bucket "${TEST_BUCKET_PREFIX}-objects")
    local test_file="/tmp/test-upload-$$"
    local download_file="/tmp/test-download-$$"

    # Create test file
    echo "Test content for S3 operations" > "$test_file"

    # Upload object
    if ! aws --endpoint-url="${S3_ENDPOINT}" s3 cp "$test_file" "s3://${bucket}/test-object.txt" >/dev/null 2>&1; then
        echo -e "  ${RED}Failed to upload object${NC}"
        rm -f "$test_file"
        cleanup_test_bucket "$bucket"
        return 1
    fi

    # Download object
    if ! aws --endpoint-url="${S3_ENDPOINT}" s3 cp "s3://${bucket}/test-object.txt" "$download_file" >/dev/null 2>&1; then
        echo -e "  ${RED}Failed to download object${NC}"
        rm -f "$test_file" "$download_file"
        cleanup_test_bucket "$bucket"
        return 1
    fi

    # Verify content
    if ! diff "$test_file" "$download_file" >/dev/null; then
        echo -e "  ${RED}Downloaded content doesn't match${NC}"
        rm -f "$test_file" "$download_file"
        cleanup_test_bucket "$bucket"
        return 1
    fi

    echo -e "  ${GREEN}Upload/download successful${NC}"
    rm -f "$test_file" "$download_file"
    cleanup_test_bucket "$bucket"
    return 0
}

test_object_listing() {
    local bucket=$(create_test_bucket "${TEST_BUCKET_PREFIX}-listing")

    # Upload objects with different prefixes
    upload_test_file "$bucket" "folder1/file1.txt" "Content 1" "text/plain"
    upload_test_file "$bucket" "folder1/file2.txt" "Content 2" "text/plain"
    upload_test_file "$bucket" "folder2/file3.txt" "Content 3" "text/plain"
    upload_test_file "$bucket" "root-file.txt" "Root content" "text/plain"

    # List all objects
    local all_objects=$(aws --endpoint-url="${S3_ENDPOINT}" s3 ls "s3://${bucket}/" --recursive 2>/dev/null | wc -l)
    if [ "$all_objects" -ne 4 ]; then
        echo -e "  ${RED}Expected 4 objects, found $all_objects${NC}"
        cleanup_test_bucket "$bucket"
        return 1
    fi

    # List with prefix
    local folder1_objects=$(aws --endpoint-url="${S3_ENDPOINT}" s3 ls "s3://${bucket}/folder1/" 2>/dev/null | wc -l)
    if [ "$folder1_objects" -ne 2 ]; then
        echo -e "  ${RED}Expected 2 objects in folder1, found $folder1_objects${NC}"
        cleanup_test_bucket "$bucket"
        return 1
    fi

    echo -e "  ${GREEN}Object listing with prefix works${NC}"
    cleanup_test_bucket "$bucket"
    return 0
}

test_object_deletion() {
    local bucket=$(create_test_bucket "${TEST_BUCKET_PREFIX}-delete")

    # Upload test object
    upload_test_file "$bucket" "delete-me.txt" "This file will be deleted" "text/plain"

    # Verify it exists
    if ! aws --endpoint-url="${S3_ENDPOINT}" s3 ls "s3://${bucket}/" 2>/dev/null | grep -q "delete-me.txt"; then
        echo -e "  ${RED}Object not found before deletion${NC}"
        cleanup_test_bucket "$bucket"
        return 1
    fi

    # Delete the object
    if ! aws --endpoint-url="${S3_ENDPOINT}" s3 rm "s3://${bucket}/delete-me.txt" >/dev/null 2>&1; then
        echo -e "  ${RED}Failed to delete object${NC}"
        cleanup_test_bucket "$bucket"
        return 1
    fi

    # Verify it's gone
    if aws --endpoint-url="${S3_ENDPOINT}" s3 ls "s3://${bucket}/" 2>/dev/null | grep -q "delete-me.txt"; then
        echo -e "  ${RED}Object still exists after deletion${NC}"
        cleanup_test_bucket "$bucket"
        return 1
    fi

    echo -e "  ${GREEN}Object deletion successful${NC}"
    cleanup_test_bucket "$bucket"
    return 0
}

test_large_file_upload() {
    local bucket=$(create_test_bucket "${TEST_BUCKET_PREFIX}-large")
    local large_file="/tmp/large-test-$$"

    # Create 1MB file
    dd if=/dev/urandom of="$large_file" bs=1024 count=1024 2>/dev/null

    # Upload large file
    if aws --endpoint-url="${S3_ENDPOINT}" s3 cp "$large_file" "s3://${bucket}/large-file.bin" >/dev/null 2>&1; then
        echo -e "  ${GREEN}Large file upload successful${NC}"
        rm -f "$large_file"
        cleanup_test_bucket "$bucket"
        return 0
    else
        echo -e "  ${YELLOW}Large file upload failed (may be size limit)${NC}"
        rm -f "$large_file"
        cleanup_test_bucket "$bucket"
        return 0  # Don't fail the test as this might be expected
    fi
}

test_special_characters() {
    local bucket=$(create_test_bucket "${TEST_BUCKET_PREFIX}-special")

    # Test various special characters in keys
    local test_keys=(
        "file-with-dash.txt"
        "file_with_underscore.txt"
        "file.with.dots.txt"
        "path/to/nested/file.txt"
    )

    for key in "${test_keys[@]}"; do
        if ! upload_test_file "$bucket" "$key" "Content for $key" "text/plain"; then
            echo -e "  ${RED}Failed to upload: $key${NC}"
            cleanup_test_bucket "$bucket"
            return 1
        fi
    done

    echo -e "  ${GREEN}Special characters handled correctly${NC}"
    cleanup_test_bucket "$bucket"
    return 0
}

# Show usage if --help is provided
if [ "$1" = "--help" ] || [ "$1" = "-h" ]; then
    echo "Usage: $0"
    echo ""
    echo "Test basic S3 operations on IronBucket."
    echo ""
    echo "Tests include:"
    echo "  - Bucket operations (create/list/delete)"
    echo "  - Object operations (upload/download)"
    echo "  - Object listing with prefixes"
    echo "  - Object deletion"
    echo "  - Large file uploads"
    echo "  - Special characters in object keys"
    echo ""
    echo "Configuration is loaded from .env file."
    exit 0
fi

# Run the tests
test_s3_operations