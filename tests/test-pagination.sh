#!/bin/bash

# Test script for IronBucket pagination functionality
# This tests list-objects-v2 with pagination, continuation tokens, and delimiters

# Don't exit on error immediately to allow test counting
set +e

# Configuration
S3_HOST="http://172.17.0.1:20000"
BUCKET="test-pagination"
AWS_ACCESS_KEY_ID="root"
AWS_SECRET_ACCESS_KEY="xxxxxxxxxxxxxxx"

# Export AWS credentials
export AWS_ACCESS_KEY_ID
export AWS_SECRET_ACCESS_KEY

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo "========================================="
echo "Testing IronBucket S3 Pagination"
echo "========================================="

# Test counter
TESTS_PASSED=0
TESTS_FAILED=0

# Function to check test result
check_result() {
    if [ $1 -eq 0 ]; then
        echo -e "${GREEN}✓ $2${NC}"
        ((TESTS_PASSED++))
    else
        echo -e "${RED}✗ $2${NC}"
        ((TESTS_FAILED++))
    fi
}

# Cleanup function
cleanup() {
    if [ -n "$TEST_COMPLETE" ]; then
        aws --endpoint-url "$S3_HOST" s3 rb "s3://$BUCKET" --force 2>/dev/null || true
    fi
}

# Ensure cleanup on exit
trap cleanup EXIT

# Mark test as not complete yet
TEST_COMPLETE=""

# Test 1: Create bucket
echo ""
echo "Test 1: Create test bucket"
aws --endpoint-url "$S3_HOST" s3 mb "s3://$BUCKET" 2>/dev/null
check_result $? "Create bucket"

# Test 2: Upload test objects
echo ""
echo "Test 2: Upload test objects"
for i in {1..10}; do
    echo "Test file $i" > "/tmp/test-pagination-$i.txt"
    aws --endpoint-url "$S3_HOST" s3 cp "/tmp/test-pagination-$i.txt" "s3://$BUCKET/test$(printf "%02d" $i).txt" --no-progress 2>/dev/null
done
check_result $? "Upload 10 test objects"

# Test 3: List with max-keys
echo ""
echo "Test 3: List objects with max-keys=3"
RESULT=$(aws --endpoint-url "$S3_HOST" s3api list-objects-v2 --bucket "$BUCKET" --max-keys 3 --output json 2>/dev/null)
COUNT=$(echo "$RESULT" | jq '.Contents | length' 2>/dev/null)
if [ "$COUNT" -eq "3" ]; then
    check_result 0 "List with max-keys=3 returned 3 objects"
else
    check_result 1 "List with max-keys=3 (expected 3, got $COUNT)"
fi

# Test 4: Check IsTruncated flag
echo ""
echo "Test 4: Check IsTruncated flag"
IS_TRUNCATED=$(echo "$RESULT" | jq -r '.IsTruncated' 2>/dev/null)
if [ "$IS_TRUNCATED" = "true" ]; then
    check_result 0 "IsTruncated is true when more objects exist"
else
    check_result 1 "IsTruncated should be true (got $IS_TRUNCATED)"
fi

# Test 5: Test continuation token
echo ""
echo "Test 5: Test continuation token"
NEXT_TOKEN=$(echo "$RESULT" | jq -r '.NextContinuationToken // empty' 2>/dev/null)
if [ -n "$NEXT_TOKEN" ]; then
    RESULT2=$(aws --endpoint-url "$S3_HOST" s3api list-objects-v2 --bucket "$BUCKET" --max-keys 3 --continuation-token "$NEXT_TOKEN" --output json 2>/dev/null)
    COUNT2=$(echo "$RESULT2" | jq '.Contents | length' 2>/dev/null)
    if [ "$COUNT2" -gt "0" ]; then
        check_result 0 "Continuation token works (got $COUNT2 more objects)"
    else
        check_result 1 "Continuation token failed"
    fi
else
    check_result 1 "No continuation token returned when truncated"
fi

# Test 6: Upload objects with folder structure
echo ""
echo "Test 6: Upload objects with folder structure"
for i in {1..3}; do
    echo "Folder test $i" > "/tmp/folder-test-$i.txt"
    aws --endpoint-url "$S3_HOST" s3 cp "/tmp/folder-test-$i.txt" "s3://$BUCKET/folder1/file$i.txt" --no-progress 2>/dev/null
    aws --endpoint-url "$S3_HOST" s3 cp "/tmp/folder-test-$i.txt" "s3://$BUCKET/folder2/file$i.txt" --no-progress 2>/dev/null
done
check_result $? "Upload objects in folder structure"

# Test 7: List with delimiter
echo ""
echo "Test 7: List with delimiter '/'"
RESULT=$(aws --endpoint-url "$S3_HOST" s3api list-objects-v2 --bucket "$BUCKET" --delimiter "/" --output json 2>/dev/null)
PREFIXES=$(echo "$RESULT" | jq '.CommonPrefixes | length' 2>/dev/null)
if [ "$PREFIXES" -eq "2" ]; then
    check_result 0 "Delimiter returns 2 common prefixes (folders)"
else
    check_result 1 "Delimiter test (expected 2 prefixes, got $PREFIXES)"
fi

# Test 8: List with prefix
echo ""
echo "Test 8: List with prefix 'folder1/'"
RESULT=$(aws --endpoint-url "$S3_HOST" s3api list-objects-v2 --bucket "$BUCKET" --prefix "folder1/" --output json 2>/dev/null)
COUNT=$(echo "$RESULT" | jq '.Contents | length' 2>/dev/null)
if [ "$COUNT" -eq "3" ]; then
    check_result 0 "Prefix filter returns correct objects"
else
    check_result 1 "Prefix filter (expected 3, got $COUNT)"
fi

# Test 9: List all objects (no pagination)
echo ""
echo "Test 9: List all objects without pagination"
RESULT=$(aws --endpoint-url "$S3_HOST" s3api list-objects-v2 --bucket "$BUCKET" --output json 2>/dev/null)
COUNT=$(echo "$RESULT" | jq '.Contents | length' 2>/dev/null)
if [ "$COUNT" -eq "16" ]; then
    check_result 0 "List all returns all 16 objects"
else
    check_result 1 "List all (expected 16, got $COUNT)"
fi

# Summary
echo ""
echo "========================================="
echo "Test Summary"
echo "========================================="
echo -e "Tests Passed: ${GREEN}$TESTS_PASSED${NC}"
echo -e "Tests Failed: ${RED}$TESTS_FAILED${NC}"

# Mark test as complete for cleanup
TEST_COMPLETE="yes"

# Final cleanup
echo ""
echo "Cleaning up test resources..."
cleanup

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}All pagination tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
fi