#!/bin/bash

# Test script for IronBucket Lifecycle functionality
# This tests object lifecycle management rules

set +e

# Source test utilities
source "$(dirname "$0")/test-utils.sh"

# Load environment
load_test_env

# Check dependencies
check_dependencies

# Initialize test environment
echo "Testing IronBucket Lifecycle Management"
echo "======================================="

check_ironbucket_running

# Configure AWS CLI for testing
export AWS_ENDPOINT_URL=${S3_ENDPOINT}

# Create aws function to use endpoint URL consistently
aws() {
    command aws --endpoint-url ${S3_ENDPOINT} "$@"
}

# Test configuration
BUCKET="test-lifecycle-$(date +%s)"

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
    rm -f /tmp/lifecycle-*.json /tmp/lifecycle-*.xml /tmp/lifecycle-get*.xml
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

# Test 2: Get lifecycle configuration (should be empty initially)
run_test "Get lifecycle configuration (empty)" "
    ! aws s3api get-bucket-lifecycle-configuration --bucket $BUCKET 2>/dev/null
"

# Test 3: Set basic lifecycle configuration
run_test "Set basic lifecycle configuration" '
cat > /tmp/lifecycle-1.json <<EOF
{
  "Rules": [
    {
      "ID": "rule1",
      "Status": "Enabled",
      "Filter": {
        "Prefix": "logs/"
      },
      "Expiration": {
        "Days": 30
      }
    }
  ]
}
EOF
aws s3api put-bucket-lifecycle-configuration --bucket $BUCKET --lifecycle-configuration file:///tmp/lifecycle-1.json
'

# Test 4: Get lifecycle configuration
run_test "Get lifecycle configuration" "
    aws s3api get-bucket-lifecycle-configuration --bucket $BUCKET > /tmp/lifecycle-get1.xml && \
    grep -q 'rule1' /tmp/lifecycle-get1.xml && \
    grep -q 'logs/' /tmp/lifecycle-get1.xml && \
    grep -q '30' /tmp/lifecycle-get1.xml
"

# Test 5: Set complex lifecycle configuration with transitions
run_test "Set complex lifecycle configuration" '
cat > /tmp/lifecycle-2.json <<EOF
{
  "Rules": [
    {
      "ID": "archive-old-data",
      "Status": "Enabled",
      "Filter": {
        "Prefix": "data/"
      },
      "Transitions": [
        {
          "Days": 30,
          "StorageClass": "STANDARD_IA"
        },
        {
          "Days": 90,
          "StorageClass": "GLACIER"
        }
      ],
      "Expiration": {
        "Days": 365
      }
    },
    {
      "ID": "delete-temp-files",
      "Status": "Enabled",
      "Filter": {
        "Prefix": "temp/"
      },
      "Expiration": {
        "Days": 7
      }
    }
  ]
}
EOF
aws s3api put-bucket-lifecycle-configuration --bucket $BUCKET --lifecycle-configuration file:///tmp/lifecycle-2.json
'

# Test 6: Verify complex lifecycle configuration
run_test "Verify complex lifecycle configuration" "
    aws s3api get-bucket-lifecycle-configuration --bucket $BUCKET > /tmp/lifecycle-get2.xml && \
    grep -q 'archive-old-data' /tmp/lifecycle-get2.xml && \
    grep -q 'delete-temp-files' /tmp/lifecycle-get2.xml && \
    grep -q 'STANDARD_IA' /tmp/lifecycle-get2.xml && \
    grep -q 'GLACIER' /tmp/lifecycle-get2.xml && \
    grep -q '365' /tmp/lifecycle-get2.xml
"

# Test 7: Update lifecycle configuration
run_test "Update lifecycle configuration" '
cat > /tmp/lifecycle-3.json <<EOF
{
  "Rules": [
    {
      "ID": "updated-rule",
      "Status": "Enabled",
      "Filter": {
        "Prefix": "archive/"
      },
      "Expiration": {
        "Days": 60
      }
    }
  ]
}
EOF
aws s3api put-bucket-lifecycle-configuration --bucket $BUCKET --lifecycle-configuration file:///tmp/lifecycle-3.json
'

# Test 8: Verify updated lifecycle configuration
run_test "Verify updated lifecycle configuration" "
    aws s3api get-bucket-lifecycle-configuration --bucket $BUCKET > /tmp/lifecycle-get3.xml && \
    grep -q 'updated-rule' /tmp/lifecycle-get3.xml && \
    grep -q 'archive/' /tmp/lifecycle-get3.xml && \
    grep -q '60' /tmp/lifecycle-get3.xml && \
    ! grep -q 'archive-old-data' /tmp/lifecycle-get3.xml
"

# Test 9: Set lifecycle with tag filter
run_test "Set lifecycle with tag filter" '
cat > /tmp/lifecycle-4.json <<EOF
{
  "Rules": [
    {
      "ID": "tag-based-rule",
      "Status": "Enabled",
      "Filter": {
        "Tag": {
          "Key": "Environment",
          "Value": "Dev"
        }
      },
      "Expiration": {
        "Days": 14
      }
    }
  ]
}
EOF
aws s3api put-bucket-lifecycle-configuration --bucket $BUCKET --lifecycle-configuration file:///tmp/lifecycle-4.json
'

# Test 10: Verify tag-based lifecycle
run_test "Verify tag-based lifecycle" "
    aws s3api get-bucket-lifecycle-configuration --bucket $BUCKET > /tmp/lifecycle-get4.xml && \
    grep -q 'tag-based-rule' /tmp/lifecycle-get4.xml && \
    grep -q 'Environment' /tmp/lifecycle-get4.xml && \
    grep -q 'Dev' /tmp/lifecycle-get4.xml
"

# Test 11: Set lifecycle with date-based expiration
run_test "Set lifecycle with date expiration" '
cat > /tmp/lifecycle-5.json <<EOF
{
  "Rules": [
    {
      "ID": "date-expiration",
      "Status": "Enabled",
      "Filter": {
        "Prefix": "project/"
      },
      "Expiration": {
        "Date": "2025-12-31T00:00:00Z"
      }
    }
  ]
}
EOF
aws s3api put-bucket-lifecycle-configuration --bucket $BUCKET --lifecycle-configuration file:///tmp/lifecycle-5.json
'

# Test 12: Verify date-based lifecycle
run_test "Verify date-based lifecycle" "
    aws s3api get-bucket-lifecycle-configuration --bucket $BUCKET > /tmp/lifecycle-get5.xml && \
    grep -q 'date-expiration' /tmp/lifecycle-get5.xml && \
    grep -q '2025-12-31' /tmp/lifecycle-get5.xml
"

# Test 13: Delete lifecycle configuration
run_test "Delete lifecycle configuration" "
    aws s3api delete-bucket-lifecycle --bucket $BUCKET
"

# Test 14: Verify lifecycle is deleted
run_test "Verify lifecycle deleted" "
    ! aws s3api get-bucket-lifecycle-configuration --bucket $BUCKET 2>/dev/null
"

# Test 15: Set lifecycle after deletion
run_test "Set lifecycle after deletion" '
cat > /tmp/lifecycle-6.json <<EOF
{
  "Rules": [
    {
      "ID": "final-rule",
      "Status": "Enabled",
      "Filter": {
        "Prefix": "final/"
      },
      "Expiration": {
        "Days": 1
      }
    }
  ]
}
EOF
aws s3api put-bucket-lifecycle-configuration --bucket $BUCKET --lifecycle-configuration file:///tmp/lifecycle-6.json
'

# Test 16: Lifecycle persistence
run_test "Lifecycle configuration persistence" '
    # Set lifecycle
cat > /tmp/lifecycle-persist.json <<EOF
{
  "Rules": [
    {
      "ID": "persistent-rule",
      "Status": "Enabled",
      "Filter": {
        "Prefix": "persist/"
      },
      "Expiration": {
        "Days": 90
      }
    }
  ]
}
EOF
    aws s3api put-bucket-lifecycle-configuration --bucket $BUCKET --lifecycle-configuration file:///tmp/lifecycle-persist.json && \
    sleep 2 && \
    # Verify it persists
    aws s3api get-bucket-lifecycle-configuration --bucket $BUCKET | grep -q "persistent-rule"
'

# Test 17: Disabled rule
run_test "Lifecycle with disabled rule" '
cat > /tmp/lifecycle-disabled.json <<EOF
{
  "Rules": [
    {
      "ID": "disabled-rule",
      "Status": "Disabled",
      "Filter": {
        "Prefix": "disabled/"
      },
      "Expiration": {
        "Days": 1
      }
    }
  ]
}
EOF
aws s3api put-bucket-lifecycle-configuration --bucket $BUCKET --lifecycle-configuration file:///tmp/lifecycle-disabled.json
'

# Test 18: Verify disabled rule
run_test "Verify disabled rule" "
    aws s3api get-bucket-lifecycle-configuration --bucket $BUCKET > /tmp/lifecycle-get-disabled.xml && \
    grep -q 'disabled-rule' /tmp/lifecycle-get-disabled.xml && \
    grep -q 'Disabled' /tmp/lifecycle-get-disabled.xml
"

# Test Summary
echo ""
echo "======================================"
echo "Test Summary:"
echo "  Total Tests: $TEST_COUNT"
echo -e "  Passed: ${GREEN}$PASS_COUNT${NC}"
echo -e "  Failed: ${RED}$FAIL_COUNT${NC}"

if [ $FAIL_COUNT -eq 0 ]; then
    echo -e "\n${GREEN}All lifecycle tests passed!${NC}"
    exit 0
else
    echo -e "\n${RED}Some lifecycle tests failed${NC}"
    exit 1
fi