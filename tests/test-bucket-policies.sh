#!/bin/bash

# Test Bucket Policies for IronBucket

set -e

# Source test utilities
source "$(dirname "$0")/test-utils.sh"

# Load environment
load_test_env

# Check dependencies
check_dependencies

# Initialize test environment
echo "Testing Bucket Policies"
echo "======================="

check_ironbucket_running

# Generate unique test bucket name
TEST_BUCKET="${TEST_BUCKET_PREFIX}-policies-$(date +%s)"

# Cleanup function
cleanup() {
    echo -e "\n${YELLOW}Cleaning up test resources...${NC}" >&2

    # Delete policy if exists
    aws s3api delete-bucket-policy --bucket "${TEST_BUCKET}" \
        --endpoint-url "$S3_ENDPOINT" 2>/dev/null || true

    # Delete all objects
    aws s3 rm "s3://${TEST_BUCKET}" --recursive --endpoint-url "$S3_ENDPOINT" 2>/dev/null || true

    # Delete bucket
    aws s3 rb "s3://${TEST_BUCKET}" --endpoint-url "$S3_ENDPOINT" 2>/dev/null || true
}

# Set trap for cleanup
trap cleanup EXIT

# Test 1: Create test bucket
echo -e "\n${YELLOW}▶ Create test bucket${NC}"
aws s3 mb "s3://${TEST_BUCKET}" --endpoint-url "$S3_ENDPOINT" --region "$S3_REGION"
echo -e "${GREEN}✓ Test bucket created${NC}"
TESTS_PASSED=$((TESTS_PASSED + 1))

# Test 2: Check no policy exists initially
echo -e "\n${YELLOW}▶ Check no policy exists initially${NC}"
POLICY_OUTPUT=$(aws s3api get-bucket-policy --bucket "${TEST_BUCKET}" \
    --endpoint-url "$S3_ENDPOINT" 2>&1 || true)

if echo "$POLICY_OUTPUT" | grep -q "NoSuchBucketPolicy"; then
    echo -e "${GREEN}✓ No policy exists initially${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ Unexpected policy output: $POLICY_OUTPUT${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 3: Set a basic bucket policy
echo -e "\n${YELLOW}▶ Set a basic bucket policy${NC}"

# Create a simple public read policy
POLICY_JSON=$(cat <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "PublicReadGetObject",
      "Effect": "Allow",
      "Principal": "*",
      "Action": "s3:GetObject",
      "Resource": "arn:aws:s3:::${TEST_BUCKET}/*"
    }
  ]
}
EOF
)

# Save policy to temp file
echo "$POLICY_JSON" > /tmp/test-policy.json

# Apply the policy
aws s3api put-bucket-policy --bucket "${TEST_BUCKET}" \
    --policy file:///tmp/test-policy.json \
    --endpoint-url "$S3_ENDPOINT"

echo -e "${GREEN}✓ Bucket policy set${NC}"
TESTS_PASSED=$((TESTS_PASSED + 1))

# Test 4: Retrieve bucket policy
echo -e "\n${YELLOW}▶ Retrieve bucket policy${NC}"
RETRIEVED_POLICY=$(aws s3api get-bucket-policy --bucket "${TEST_BUCKET}" \
    --endpoint-url "$S3_ENDPOINT" --output text 2>/dev/null)

if echo "$RETRIEVED_POLICY" | grep -q "PublicReadGetObject"; then
    echo -e "${GREEN}✓ Policy retrieved successfully${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ Retrieved policy doesn't match${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 5: Update bucket policy
echo -e "\n${YELLOW}▶ Update bucket policy${NC}"

# Create an updated policy with more permissions
UPDATED_POLICY_JSON=$(cat <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "PublicReadGetObject",
      "Effect": "Allow",
      "Principal": "*",
      "Action": ["s3:GetObject", "s3:ListBucket"],
      "Resource": ["arn:aws:s3:::${TEST_BUCKET}/*", "arn:aws:s3:::${TEST_BUCKET}"]
    }
  ]
}
EOF
)

echo "$UPDATED_POLICY_JSON" > /tmp/test-policy-updated.json

# Apply the updated policy
aws s3api put-bucket-policy --bucket "${TEST_BUCKET}" \
    --policy file:///tmp/test-policy-updated.json \
    --endpoint-url "$S3_ENDPOINT"

# Verify updated policy
UPDATED_RETRIEVED=$(aws s3api get-bucket-policy --bucket "${TEST_BUCKET}" \
    --endpoint-url "$S3_ENDPOINT" --output text 2>/dev/null)

if echo "$UPDATED_RETRIEVED" | grep -q "s3:ListBucket"; then
    echo -e "${GREEN}✓ Policy updated successfully${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ Updated policy not found${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 6: Test policy with specific principal
echo -e "\n${YELLOW}▶ Set policy with specific principal${NC}"

PRINCIPAL_POLICY_JSON=$(cat <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "SpecificUserAccess",
      "Effect": "Allow",
      "Principal": {
        "AWS": "arn:aws:iam::123456789012:root"
      },
      "Action": "s3:*",
      "Resource": "arn:aws:s3:::${TEST_BUCKET}/*"
    }
  ]
}
EOF
)

echo "$PRINCIPAL_POLICY_JSON" > /tmp/test-policy-principal.json

aws s3api put-bucket-policy --bucket "${TEST_BUCKET}" \
    --policy file:///tmp/test-policy-principal.json \
    --endpoint-url "$S3_ENDPOINT"

PRINCIPAL_POLICY=$(aws s3api get-bucket-policy --bucket "${TEST_BUCKET}" \
    --endpoint-url "$S3_ENDPOINT" --output text 2>/dev/null)

if echo "$PRINCIPAL_POLICY" | grep -q "arn:aws:iam::123456789012:root"; then
    echo -e "${GREEN}✓ Policy with specific principal set${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ Principal policy not set correctly${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 7: Test policy with deny effect
echo -e "\n${YELLOW}▶ Set policy with deny effect${NC}"

DENY_POLICY_JSON=$(cat <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "DenyUnencryptedObjectUploads",
      "Effect": "Deny",
      "Principal": "*",
      "Action": "s3:PutObject",
      "Resource": "arn:aws:s3:::${TEST_BUCKET}/*",
      "Condition": {
        "StringNotEquals": {
          "s3:x-amz-server-side-encryption": "AES256"
        }
      }
    }
  ]
}
EOF
)

echo "$DENY_POLICY_JSON" > /tmp/test-policy-deny.json

aws s3api put-bucket-policy --bucket "${TEST_BUCKET}" \
    --policy file:///tmp/test-policy-deny.json \
    --endpoint-url "$S3_ENDPOINT"

DENY_POLICY=$(aws s3api get-bucket-policy --bucket "${TEST_BUCKET}" \
    --endpoint-url "$S3_ENDPOINT" --output text 2>/dev/null)

if echo "$DENY_POLICY" | grep -q '"Effect": "Deny"' || echo "$DENY_POLICY" | grep -q 'Effect.*Deny'; then
    echo -e "${GREEN}✓ Policy with deny effect set${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ Deny policy not set correctly${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 8: Delete bucket policy
echo -e "\n${YELLOW}▶ Delete bucket policy${NC}"
aws s3api delete-bucket-policy --bucket "${TEST_BUCKET}" \
    --endpoint-url "$S3_ENDPOINT"

# Verify policy is deleted
DELETED_CHECK=$(aws s3api get-bucket-policy --bucket "${TEST_BUCKET}" \
    --endpoint-url "$S3_ENDPOINT" 2>&1 || true)

if echo "$DELETED_CHECK" | grep -q "NoSuchBucketPolicy"; then
    echo -e "${GREEN}✓ Policy deleted successfully${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ Policy still exists after deletion${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 9: Try to delete non-existent policy
echo -e "\n${YELLOW}▶ Try to delete non-existent policy${NC}"
DELETE_NONEXISTENT=$(aws s3api delete-bucket-policy --bucket "${TEST_BUCKET}" \
    --endpoint-url "$S3_ENDPOINT" 2>&1 || true)

if echo "$DELETE_NONEXISTENT" | grep -q "NoSuchBucketPolicy" || [ -z "$DELETE_NONEXISTENT" ]; then
    echo -e "${GREEN}✓ Handled deleting non-existent policy${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ Unexpected response: $DELETE_NONEXISTENT${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 10: Test invalid JSON policy
echo -e "\n${YELLOW}▶ Test invalid JSON policy${NC}"
INVALID_JSON='{ "Version": "2012-10-17", invalid json }'

echo "$INVALID_JSON" > /tmp/test-policy-invalid.json

INVALID_RESULT=$(aws s3api put-bucket-policy --bucket "${TEST_BUCKET}" \
    --policy file:///tmp/test-policy-invalid.json \
    --endpoint-url "$S3_ENDPOINT" 2>&1 || true)

if echo "$INVALID_RESULT" | grep -q -E "(MalformedPolicy|Invalid|Error|Failed)"; then
    echo -e "${GREEN}✓ Invalid JSON rejected${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${YELLOW}⚠ Invalid JSON might not be properly validated${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 11: Test policy persistence
echo -e "\n${YELLOW}▶ Test policy persistence${NC}"

# Set a policy
PERSIST_POLICY_JSON=$(cat <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "PersistenceTest",
      "Effect": "Allow",
      "Principal": "*",
      "Action": "s3:GetObject",
      "Resource": "arn:aws:s3:::${TEST_BUCKET}/*"
    }
  ]
}
EOF
)

echo "$PERSIST_POLICY_JSON" > /tmp/test-policy-persist.json

aws s3api put-bucket-policy --bucket "${TEST_BUCKET}" \
    --policy file:///tmp/test-policy-persist.json \
    --endpoint-url "$S3_ENDPOINT"

# Wait a moment
sleep 2

# Retrieve policy again
PERSISTED_POLICY=$(aws s3api get-bucket-policy --bucket "${TEST_BUCKET}" \
    --endpoint-url "$S3_ENDPOINT" --output text 2>/dev/null)

if echo "$PERSISTED_POLICY" | grep -q "PersistenceTest"; then
    echo -e "${GREEN}✓ Policy persisted correctly${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ Policy not persisted${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test 12: Test complex policy with multiple statements
echo -e "\n${YELLOW}▶ Test complex policy with multiple statements${NC}"

COMPLEX_POLICY_JSON=$(cat <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "PublicRead",
      "Effect": "Allow",
      "Principal": "*",
      "Action": "s3:GetObject",
      "Resource": "arn:aws:s3:::${TEST_BUCKET}/public/*"
    },
    {
      "Sid": "AdminFullAccess",
      "Effect": "Allow",
      "Principal": {
        "AWS": "arn:aws:iam::123456789012:user/admin"
      },
      "Action": "s3:*",
      "Resource": ["arn:aws:s3:::${TEST_BUCKET}", "arn:aws:s3:::${TEST_BUCKET}/*"]
    },
    {
      "Sid": "DenyDeleteForAll",
      "Effect": "Deny",
      "Principal": "*",
      "Action": "s3:DeleteObject",
      "Resource": "arn:aws:s3:::${TEST_BUCKET}/protected/*"
    }
  ]
}
EOF
)

echo "$COMPLEX_POLICY_JSON" > /tmp/test-policy-complex.json

aws s3api put-bucket-policy --bucket "${TEST_BUCKET}" \
    --policy file:///tmp/test-policy-complex.json \
    --endpoint-url "$S3_ENDPOINT"

COMPLEX_RETRIEVED=$(aws s3api get-bucket-policy --bucket "${TEST_BUCKET}" \
    --endpoint-url "$S3_ENDPOINT" --output text 2>/dev/null)

if echo "$COMPLEX_RETRIEVED" | grep -q "PublicRead" && \
   echo "$COMPLEX_RETRIEVED" | grep -q "AdminFullAccess" && \
   echo "$COMPLEX_RETRIEVED" | grep -q "DenyDeleteForAll"; then
    echo -e "${GREEN}✓ Complex policy with multiple statements set${NC}"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ Complex policy not set correctly${NC}"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Print test summary
print_summary