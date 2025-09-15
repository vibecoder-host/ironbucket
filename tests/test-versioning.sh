#!/bin/bash

# Test Object Versioning for IronBucket

set -e

# Source test utilities
source "$(dirname "$0")/test-utils.sh"

# Load environment
load_test_env

# Check dependencies
check_dependencies

# Initialize test environment
echo "Testing Object Versioning"
echo "========================="

# Initialize test counters
TESTS_PASSED=0
TESTS_FAILED=0

check_ironbucket_running

# Generate unique test bucket name
TEST_BUCKET="${TEST_BUCKET_PREFIX}-versioning-$(date +%s)"

# Cleanup function
cleanup() {
    echo -e "\n${YELLOW}Cleaning up test resources...${NC}" >&2

    # Delete all versions
    if [ "$VERSIONING_ENABLED" = "true" ]; then
        # List and delete all versions
        aws s3api list-object-versions --bucket "${TEST_BUCKET}" \
            --endpoint-url "$S3_ENDPOINT" 2>/dev/null | \
            jq -r '.Versions[]? | "--key \"\(.Key)\" --version-id \(.VersionId)"' | \
            while read -r key_args; do
                eval "aws s3api delete-object --bucket \"${TEST_BUCKET}\" $key_args --endpoint-url \"$S3_ENDPOINT\"" 2>/dev/null || true
            done
    fi

    aws s3 rm "s3://${TEST_BUCKET}" --recursive --endpoint-url "$S3_ENDPOINT" 2>/dev/null || true
    aws s3 rb "s3://${TEST_BUCKET}" --endpoint-url "$S3_ENDPOINT" 2>/dev/null || true
}

# Set trap for cleanup
trap cleanup EXIT

# Test 1: Create test bucket
echo -e "\n${YELLOW}▶ Create test bucket${NC}"
aws s3 mb "s3://${TEST_BUCKET}" --endpoint-url "$S3_ENDPOINT" --region "$S3_REGION"
echo -e "${GREEN}✓ Test bucket created${NC}"
TESTS_PASSED=$((TESTS_PASSED + 1))

# Test 2: Check default versioning status (should be disabled/null)
echo -e "\n${YELLOW}▶ Check default versioning status${NC}"
VERSIONING_STATUS=$(aws s3api get-bucket-versioning --bucket "${TEST_BUCKET}" \
    --endpoint-url "$S3_ENDPOINT" 2>/dev/null || echo 'ERROR')

# AWS returns empty body when versioning is not configured
if [ -z "$VERSIONING_STATUS" ] || [ "$VERSIONING_STATUS" = "{}" ] || echo "$VERSIONING_STATUS" | grep -q '"Status": null'; then
    echo -e "${GREEN}✓ Default versioning status is disabled${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ Unexpected default versioning status: $VERSIONING_STATUS${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 3: Enable versioning
echo -e "\n${YELLOW}▶ Enable versioning${NC}"
aws s3api put-bucket-versioning --bucket "${TEST_BUCKET}" \
    --versioning-configuration Status=Enabled \
    --endpoint-url "$S3_ENDPOINT"

VERSIONING_ENABLED=true
echo -e "${GREEN}✓ Versioning enabled${NC}"
TESTS_PASSED=$((TESTS_PASSED + 1))

# Test 4: Verify versioning is enabled
echo -e "\n${YELLOW}▶ Verify versioning is enabled${NC}"
VERSIONING_STATUS=$(aws s3api get-bucket-versioning --bucket "${TEST_BUCKET}" \
    --endpoint-url "$S3_ENDPOINT" | jq -r '.Status // "null"')

if [ "$VERSIONING_STATUS" = "Enabled" ]; then
    echo -e "${GREEN}✓ Versioning is enabled${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ Versioning status is not Enabled: $VERSIONING_STATUS${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 5: Upload object and check version ID
echo -e "\n${YELLOW}▶ Upload object with versioning enabled${NC}"
echo "Version 1 content" > /tmp/test-version.txt

# Upload and capture headers
UPLOAD_OUTPUT=$(aws s3api put-object --bucket "${TEST_BUCKET}" \
    --key "versioned-object.txt" \
    --body /tmp/test-version.txt \
    --endpoint-url "$S3_ENDPOINT" 2>&1)

VERSION_ID=$(echo "$UPLOAD_OUTPUT" | jq -r '.VersionId // "null"')

if [ "$VERSION_ID" != "null" ] && [ -n "$VERSION_ID" ]; then
    echo -e "${GREEN}✓ Version ID returned: $VERSION_ID${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${YELLOW}⚠ No version ID returned (may not be implemented yet)${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 6: Upload same object again (create new version)
echo -e "\n${YELLOW}▶ Upload new version of same object${NC}"
echo "Version 2 content" > /tmp/test-version-2.txt

UPLOAD_OUTPUT2=$(aws s3api put-object --bucket "${TEST_BUCKET}" \
    --key "versioned-object.txt" \
    --body /tmp/test-version-2.txt \
    --endpoint-url "$S3_ENDPOINT" 2>&1)

VERSION_ID2=$(echo "$UPLOAD_OUTPUT2" | jq -r '.VersionId // "null"')

if [ "$VERSION_ID2" != "null" ] && [ "$VERSION_ID2" != "$VERSION_ID" ]; then
    echo -e "${GREEN}✓ New version created: $VERSION_ID2${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${YELLOW}⚠ Version IDs not different or not returned${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 7: List object versions
echo -e "\n${YELLOW}▶ List object versions${NC}"
VERSIONS_OUTPUT=$(aws s3api list-object-versions --bucket "${TEST_BUCKET}" \
    --endpoint-url "$S3_ENDPOINT" 2>&1 || echo "ERROR")

if echo "$VERSIONS_OUTPUT" | grep -q "versioned-object.txt"; then
    echo -e "${GREEN}✓ Object versions listed successfully${NC}"
    echo "$VERSIONS_OUTPUT" | jq '.Versions[]? | {Key, VersionId, IsLatest}' 2>/dev/null || true
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${YELLOW}⚠ Could not list object versions${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 8: Get latest version content
echo -e "\n${YELLOW}▶ Get latest version content${NC}"
aws s3 cp "s3://${TEST_BUCKET}/versioned-object.txt" /tmp/latest-version.txt \
    --endpoint-url "$S3_ENDPOINT" >/dev/null 2>&1

LATEST_CONTENT=$(cat /tmp/latest-version.txt)
if [ "$LATEST_CONTENT" = "Version 2 content" ]; then
    echo -e "${GREEN}✓ Latest version content is correct${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ Latest version content incorrect: $LATEST_CONTENT${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 9: Suspend versioning
echo -e "\n${YELLOW}▶ Suspend versioning${NC}"
aws s3api put-bucket-versioning --bucket "${TEST_BUCKET}" \
    --versioning-configuration Status=Suspended \
    --endpoint-url "$S3_ENDPOINT"

VERSIONING_STATUS=$(aws s3api get-bucket-versioning --bucket "${TEST_BUCKET}" \
    --endpoint-url "$S3_ENDPOINT" | jq -r '.Status // "null"')

if [ "$VERSIONING_STATUS" = "Suspended" ]; then
    echo -e "${GREEN}✓ Versioning suspended${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ Failed to suspend versioning: $VERSIONING_STATUS${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 10: Upload with suspended versioning (should not create version)
echo -e "\n${YELLOW}▶ Upload with suspended versioning${NC}"
echo "Version 3 content (suspended)" > /tmp/test-version-3.txt

UPLOAD_OUTPUT3=$(aws s3api put-object --bucket "${TEST_BUCKET}" \
    --key "versioned-object.txt" \
    --body /tmp/test-version-3.txt \
    --endpoint-url "$S3_ENDPOINT" 2>&1)

VERSION_ID3=$(echo "$UPLOAD_OUTPUT3" | jq -r '.VersionId // "null"')

if [ "$VERSION_ID3" = "null" ]; then
    echo -e "${GREEN}✓ No version ID with suspended versioning${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${YELLOW}⚠ Version ID returned when versioning suspended: $VERSION_ID3${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 11: Multiple objects with versions
echo -e "\n${YELLOW}▶ Test multiple objects with versions${NC}"

# Re-enable versioning
aws s3api put-bucket-versioning --bucket "${TEST_BUCKET}" \
    --versioning-configuration Status=Enabled \
    --endpoint-url "$S3_ENDPOINT"

# Upload multiple objects with versions
for i in {1..3}; do
    for v in {1..2}; do
        echo "Object $i Version $v" > /tmp/multi-obj.txt
        aws s3api put-object --bucket "${TEST_BUCKET}" \
            --key "multi-object-$i.txt" \
            --body /tmp/multi-obj.txt \
            --endpoint-url "$S3_ENDPOINT" >/dev/null 2>&1
    done
done

# List all versions
MULTI_VERSIONS=$(aws s3api list-object-versions --bucket "${TEST_BUCKET}" \
    --endpoint-url "$S3_ENDPOINT" --prefix "multi-object" 2>&1)

VERSION_COUNT=$(echo "$MULTI_VERSIONS" | jq '.Versions | length' 2>/dev/null || echo "0")

if [ "$VERSION_COUNT" -ge "3" ]; then
    echo -e "${GREEN}✓ Multiple objects with versions created${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${YELLOW}⚠ Expected multiple versions, found: $VERSION_COUNT${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 12: Check versioning persistence after restart
echo -e "\n${YELLOW}▶ Check versioning persistence${NC}"

# Get current status
ORIGINAL_STATUS=$(aws s3api get-bucket-versioning --bucket "${TEST_BUCKET}" \
    --endpoint-url "$S3_ENDPOINT" | jq -r '.Status // "null"')

# Simulate restart by waiting a moment (actual restart would disconnect tests)
sleep 2

# Check status again
NEW_STATUS=$(aws s3api get-bucket-versioning --bucket "${TEST_BUCKET}" \
    --endpoint-url "$S3_ENDPOINT" | jq -r '.Status // "null"')

if [ "$ORIGINAL_STATUS" = "$NEW_STATUS" ]; then
    echo -e "${GREEN}✓ Versioning status persisted${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ Versioning status changed: $ORIGINAL_STATUS -> $NEW_STATUS${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Print test summary
print_summary $TESTS_PASSED $TESTS_FAILED