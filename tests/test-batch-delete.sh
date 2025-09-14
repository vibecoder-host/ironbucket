#!/bin/bash

# Test Batch Delete Operations for IronBucket

set -e

# Source test utilities
source "$(dirname "$0")/test-utils.sh"

# Load environment
load_test_env

# Check dependencies
check_dependencies

# Initialize test environment
echo "Testing Batch Delete Operations"
echo "==============================="

check_ironbucket_running

# Generate unique test bucket name
TEST_BUCKET="${TEST_BUCKET_PREFIX}-batch-delete-$(date +%s)"

# Cleanup function
cleanup() {
    echo -e "\n${YELLOW}Cleaning up test resources...${NC}" >&2
    aws s3 rm "s3://${TEST_BUCKET}" --recursive --endpoint-url "$S3_ENDPOINT" 2>/dev/null || true
    aws s3 rb "s3://${TEST_BUCKET}" --endpoint-url "$S3_ENDPOINT" 2>/dev/null || true
}

# Set trap for cleanup
trap cleanup EXIT

# Test 1: Create test bucket
echo -e "\n${YELLOW}▶ Create test bucket${NC}"
aws s3 mb "s3://${TEST_BUCKET}" --endpoint-url "$S3_ENDPOINT" --region "$S3_REGION"
echo -e "${GREEN}✓ Test bucket created${NC}"

# Test 2: Upload test objects
echo -e "\n${YELLOW}▶ Upload test objects for deletion${NC}"
echo 'Test content 1' > /tmp/test-delete-1.txt
echo 'Test content 2' > /tmp/test-delete-2.txt
echo 'Test content 3' > /tmp/test-delete-3.txt
echo 'Keep this file' > /tmp/test-keep.txt

aws s3 cp /tmp/test-delete-1.txt "s3://${TEST_BUCKET}/delete-1.txt" --endpoint-url "$S3_ENDPOINT"
aws s3 cp /tmp/test-delete-2.txt "s3://${TEST_BUCKET}/delete-2.txt" --endpoint-url "$S3_ENDPOINT"
aws s3 cp /tmp/test-delete-3.txt "s3://${TEST_BUCKET}/delete-3.txt" --endpoint-url "$S3_ENDPOINT"
aws s3 cp /tmp/test-keep.txt "s3://${TEST_BUCKET}/keep.txt" --endpoint-url "$S3_ENDPOINT"

OBJECT_COUNT=$(aws s3 ls "s3://${TEST_BUCKET}/" --endpoint-url "$S3_ENDPOINT" | wc -l)
if [ "$OBJECT_COUNT" -eq 4 ]; then
    echo -e "${GREEN}✓ All 4 test objects uploaded${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ Expected 4 objects, found $OBJECT_COUNT${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 3: Batch delete multiple objects
echo -e "\n${YELLOW}▶ Batch delete multiple objects${NC}"

# Create delete request XML
cat > /tmp/delete-request.xml <<EOF
<?xml version='1.0' encoding='UTF-8'?>
<Delete xmlns='http://s3.amazonaws.com/doc/2006-03-01/'>
    <Object>
        <Key>delete-1.txt</Key>
    </Object>
    <Object>
        <Key>delete-2.txt</Key>
    </Object>
    <Object>
        <Key>delete-3.txt</Key>
    </Object>
</Delete>
EOF

# Send batch delete request
RESPONSE=$(curl -s -X POST \
    -H "Content-Type: application/xml" \
    -H "Content-MD5: $(openssl dgst -md5 -binary /tmp/delete-request.xml | openssl enc -base64)" \
    --data-binary @/tmp/delete-request.xml \
    --user "${S3_ACCESS_KEY}:${S3_SECRET_KEY}" \
    --aws-sigv4 "aws:amz:${S3_REGION}:s3" \
    "${S3_ENDPOINT}/${TEST_BUCKET}?delete")

echo "Response: $RESPONSE"

if echo "$RESPONSE" | grep -q '<DeleteResult'; then
    # Verify objects were deleted
    REMAINING=$(aws s3 ls "s3://${TEST_BUCKET}/" --endpoint-url "$S3_ENDPOINT" | wc -l)
    if [ "$REMAINING" -eq 1 ]; then
        echo -e "${GREEN}✓ Batch delete successful - 3 objects deleted, 1 remains${NC}"
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        echo -e "${RED}✗ Expected 1 remaining object, found $REMAINING${NC}"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
else
    echo -e "${RED}✗ Batch delete response invalid${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 4: Verify kept object still exists
echo -e "\n${YELLOW}▶ Verify non-deleted object remains${NC}"
if aws s3 ls "s3://${TEST_BUCKET}/keep.txt" --endpoint-url "$S3_ENDPOINT" >/dev/null 2>&1; then
    echo -e "${GREEN}✓ Kept object still exists${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ Kept object was incorrectly deleted${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 5: Batch delete with non-existent objects
echo -e "\n${YELLOW}▶ Batch delete with errors (non-existent objects)${NC}"

cat > /tmp/delete-error.xml <<EOF
<?xml version='1.0' encoding='UTF-8'?>
<Delete xmlns='http://s3.amazonaws.com/doc/2006-03-01/'>
    <Object>
        <Key>does-not-exist-1.txt</Key>
    </Object>
    <Object>
        <Key>does-not-exist-2.txt</Key>
    </Object>
    <Object>
        <Key>keep.txt</Key>
    </Object>
</Delete>
EOF

RESPONSE=$(curl -s -X POST \
    -H "Content-Type: application/xml" \
    -H "Content-MD5: $(openssl dgst -md5 -binary /tmp/delete-error.xml | openssl enc -base64)" \
    --data-binary @/tmp/delete-error.xml \
    --user "${S3_ACCESS_KEY}:${S3_SECRET_KEY}" \
    --aws-sigv4 "aws:amz:${S3_REGION}:s3" \
    "${S3_ENDPOINT}/${TEST_BUCKET}?delete")

echo "Response: $RESPONSE"

if echo "$RESPONSE" | grep -q '<Error>' && echo "$RESPONSE" | grep -q '<Deleted>'; then
    echo -e "${GREEN}✓ Batch delete handles errors and successes correctly${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ Batch delete error handling incorrect${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 6: Empty batch delete request
echo -e "\n${YELLOW}▶ Empty batch delete request${NC}"

cat > /tmp/delete-empty.xml <<EOF
<?xml version='1.0' encoding='UTF-8'?>
<Delete xmlns='http://s3.amazonaws.com/doc/2006-03-01/'>
</Delete>
EOF

RESPONSE=$(curl -s -X POST \
    -H "Content-Type: application/xml" \
    -H "Content-MD5: $(openssl dgst -md5 -binary /tmp/delete-empty.xml | openssl enc -base64)" \
    --data-binary @/tmp/delete-empty.xml \
    --user "${S3_ACCESS_KEY}:${S3_SECRET_KEY}" \
    --aws-sigv4 "aws:amz:${S3_REGION}:s3" \
    "${S3_ENDPOINT}/${TEST_BUCKET}?delete")

if echo "$RESPONSE" | grep -q '<DeleteResult'; then
    echo -e "${GREEN}✓ Empty batch delete handled correctly${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ Invalid response for empty batch delete${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 7: Large batch delete
echo -e "\n${YELLOW}▶ Large batch delete operation${NC}"

# Create many test files
echo "Creating 20 test files..."
for i in {1..20}; do
    echo "Content $i" > "/tmp/bulk-$i.txt"
    aws s3 cp "/tmp/bulk-$i.txt" "s3://${TEST_BUCKET}/bulk-$i.txt" --endpoint-url "$S3_ENDPOINT" >/dev/null 2>&1
done

# Build delete request for all bulk files
echo '<?xml version="1.0" encoding="UTF-8"?>' > /tmp/delete-bulk.xml
echo '<Delete xmlns="http://s3.amazonaws.com/doc/2006-03-01/">' >> /tmp/delete-bulk.xml
for i in {1..20}; do
    echo "    <Object><Key>bulk-$i.txt</Key></Object>" >> /tmp/delete-bulk.xml
done
echo '</Delete>' >> /tmp/delete-bulk.xml

# Send large batch delete request
RESPONSE=$(curl -s -X POST \
    -H "Content-Type: application/xml" \
    -H "Content-MD5: $(openssl dgst -md5 -binary /tmp/delete-bulk.xml | openssl enc -base64)" \
    --data-binary @/tmp/delete-bulk.xml \
    --user "${S3_ACCESS_KEY}:${S3_SECRET_KEY}" \
    --aws-sigv4 "aws:amz:${S3_REGION}:s3" \
    "${S3_ENDPOINT}/${TEST_BUCKET}?delete")

DELETED_COUNT=$(echo "$RESPONSE" | grep -o '<Deleted>' | wc -l)

if [ "$DELETED_COUNT" -eq 20 ]; then
    echo -e "${GREEN}✓ All 20 objects deleted successfully${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ Expected 20 deletions, got $DELETED_COUNT${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Print test summary
print_summary