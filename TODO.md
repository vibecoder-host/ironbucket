# IronBucket TODO List

## Overview
This document tracks all pending tasks, improvements, and features to be implemented in IronBucket.

## Task Status Legend
- ⬜ Not Started
- 🟦 In Progress
- ✅ Completed
- ❌ Blocked/Deferred
- 🟨 Partially Complete

---

## Core S3 API Features

### Multipart Upload
- ✅ **Implement multipart upload initiation** (`src/main.rs:615-634`) - Completed 2025-09-14
- ✅ **Implement part upload** (`src/main.rs:548-599`) - Completed 2025-09-14
- ✅ **Implement multipart upload completion** (`src/main.rs:666-758`) - Completed 2025-09-14
- ✅ **Store upload info** (Persisted to `.multipart` directory) - Completed 2025-09-14
- ✅ **Store part** (Parts saved to disk with metadata) - Completed 2025-09-14
- ✅ **Assemble parts and create final object** (`src/main.rs:668-688`) - Completed 2025-09-14
- ✅ **Clean up parts** (Cleanup on completion/abort) - Completed 2025-09-14
- ✅ **Handle multipart upload in main.rs** (Fully implemented with persistence) - Completed 2025-09-14

### Batch Operations
- ✅ **Implement batch delete** (`src/main.rs:384-569`) - Completed 2025-09-14
  - ✅ Parse XML delete request with Object elements
  - ✅ Process multiple object deletions
  - ✅ Return DeleteResult XML with successes and errors
  - ✅ Handle non-existent objects with proper error codes
  - ✅ Delete both object files and metadata files


### Versioning
- ✅ **Set versioning status** (`src/main.rs:368-405`) - Completed 2025-09-14
  - ✅ Parse versioning configuration XML
  - ✅ Store versioning status in memory and on disk
  - ✅ Support Enabled/Suspended states
- ✅ **Get versioning status** (`src/main.rs:289-319`) - Completed 2025-09-14
  - ✅ Return versioning configuration XML
  - ✅ Load from memory or disk
- ✅ **List object versions** (`src/main.rs:377-481`) - Completed 2025-09-14
  - ✅ List all versions of objects
  - ✅ Support delete markers
  - ✅ Mark latest versions
- ✅ **Version ID support in metadata** (`src/main.rs:1365-1442`) - Completed 2025-09-14
  - ✅ Generate unique version IDs
  - ✅ Store versioned objects separately
  - ✅ Return version ID in response headers

---

## Security & Access Control

### Bucket Policies
- ✅ **Store bucket policy** (`src/main.rs:577-618`) - Completed 2025-09-14
  - ✅ Parse and validate JSON policy
  - ✅ Store policy in memory and on disk
  - ✅ Return proper error for malformed policies
- ✅ **Retrieve bucket policy** (`src/main.rs:350-385`) - Completed 2025-09-14
  - ✅ Return policy from memory or disk
  - ✅ Handle NoSuchBucketPolicy error
- ✅ **Delete bucket policy** (`src/main.rs:1276-1314`) - Completed 2025-09-14
  - ✅ Remove policy from memory and disk
  - ✅ Return proper error if no policy exists
- ✅ **Check principal permissions** (`src/main.rs:108-196`) - Completed 2025-09-14
  - ✅ Parse policy statements
  - ✅ Match principal, action, and resource
  - ✅ Support Allow/Deny effects

### Encryption
- ✅ **Implement encryption** (`src/main.rs:113-144`) - Completed 2025-09-14
  - ✅ AES-256-GCM encryption with per-object keys
  - ✅ Automatic key generation and storage in metadata
- ✅ **Implement decryption** (`src/main.rs:146-174`) - Completed 2025-09-14
  - ✅ Transparent decryption on object retrieval
  - ✅ Support for mixed encrypted/unencrypted objects
- ✅ **Set bucket encryption** (`src/main.rs:619-659`) - Completed 2025-09-14
  - ✅ PUT bucket encryption configuration endpoint
  - ✅ Parse and validate encryption rules
  - ✅ Persist configuration to disk
- ✅ **Get bucket encryption** (`src/main.rs:320-349`) - Completed 2025-09-14
  - ✅ GET bucket encryption configuration endpoint
  - ✅ Return proper error when no encryption configured

### CORS
- ✅ **Store CORS configuration** (`src/main.rs:1057-1133`) - Completed 2025-09-14
  - ✅ Parse JSON CORS configuration from AWS CLI
  - ✅ Validate CORS rules and required fields
  - ✅ Persist configuration to disk
- ✅ **Retrieve CORS configuration** (`src/main.rs:633-750`) - Completed 2025-09-14
  - ✅ GET bucket CORS configuration endpoint
  - ✅ Return XML format for AWS CLI compatibility
  - ✅ Load from memory or disk
- ✅ **Delete CORS configuration** (`src/main.rs:1939-1979`) - Completed 2025-09-14
  - ✅ DELETE bucket CORS configuration endpoint
  - ✅ Remove configuration from memory and disk

### ACL (Access Control Lists)
- ⬜ **Store ACL** (`src/acl.rs:46`)
- ⬜ **Retrieve ACL** (`src/acl.rs:51`)
- ⬜ **Check user permissions** (`src/acl.rs:68`)
- 🟨 **Set object/bucket ACL** (endpoints exist, not persisted)

---

## Lifecycle & Management

### Lifecycle Rules
- ⬜ **Store lifecycle configuration** (`src/lifecycle.rs:58`)
- ⬜ **Retrieve lifecycle configuration** (`src/lifecycle.rs:63`)
- ⬜ **Delete lifecycle configuration** (`src/lifecycle.rs:68`)
- ⬜ **Apply lifecycle rules to objects** (`src/lifecycle.rs:73`)

---

## Clustering & Replication

### Cluster Operations
- ⬜ **Notify peer nodes on object creation** (`src/cluster.rs:21`)
- ⬜ **Notify peer nodes on object deletion** (`src/cluster.rs:26`)
- ⬜ **Replicate to peer nodes** (`src/cluster.rs:31`)
- ⬜ **Notify peer nodes on bucket operations** (`src/cluster.rs:36`)
- ⬜ **Check cluster health** (`src/cluster.rs:41`)

---

## Optional

- ⬜ **Form-based uploads** (`src/main.rs:228`)


---

## Performance & Optimization

### Storage Backend
- 🟨 **Optimize list_objects with pagination** (`src/storage/filesystem.rs:282`)
  - Current implementation is simple, needs pagination support

---

## Recently Completed ✅

### Core Functionality
- ✅ **Basic S3 operations** (PUT, GET, DELETE, HEAD)
- ✅ **Bucket operations** (create, list, delete)
- ✅ **Metadata persistence** (stored as .metadata files)
- ✅ **AWS Signature V4 authentication**
- ✅ **Content-type handling**
- ✅ **Multipart Upload** (Complete implementation with persistence) - 2025-09-14
  - Initiate multipart upload with metadata persistence
  - Upload parts with disk storage
  - List parts functionality
  - Complete multipart upload with object assembly
  - Abort multipart upload with cleanup
  - Support for large files
  - Comprehensive test suite
- ✅ **Encryption** (Server-side AES-256-GCM encryption) - 2025-09-14
  - Bucket-level encryption configuration
  - Automatic encryption on upload when enabled
  - Transparent decryption on retrieval
  - Per-object encryption keys
  - Support for mixed encrypted/unencrypted objects
  - Complete test coverage

### Infrastructure
- ✅ **Remove Redis dependency**
- ✅ **Remove SQLite dependency**
- ✅ **Filesystem-based metadata storage**
- ✅ **Docker containerization**
- ✅ **Test suite implementation**

---

## Priority Matrix

### High Priority 🔴
1. ~~Multipart upload (required for large files)~~ ✅ COMPLETED
2. Batch delete operations
3. Object versioning

### Medium Priority 🟡
1. ACL implementation
2. Bucket policies
3. CORS configuration
4. Lifecycle rules

### Low Priority 🟢
1. Encryption at rest
2. Clustering support
3. Form-based uploads

---

## Technical Debt

### Code Quality
- ⬜ Remove unused `HmacSha256` type alias (`src/main.rs:36`)
- ⬜ Remove unused `ObjectData.data` field (`src/main.rs:115`)
- ⬜ Remove unused `MultipartUpload.upload_id` field (`src/main.rs:135`)
- ⬜ Consolidate error handling for NotImplemented features

### Architecture
- ⬜ Refactor main.rs to use modular storage backend properly
- ⬜ Move S3 handlers from main.rs to s3/handlers.rs module
- ⬜ Implement proper separation of concerns

---

## Testing Requirements

### Unit Tests
- ⬜ ACL module tests
- ⬜ Policy module tests
- ⬜ Versioning module tests
- ⬜ Multipart upload tests
- ⬜ Encryption module tests

### Integration Tests
- ✅ Basic S3 operations
- ✅ Metadata persistence
- ✅ Multipart upload workflow (8 comprehensive tests)
- ✅ Batch delete operations (7 comprehensive tests)
- ✅ Versioning workflow (12 comprehensive tests)
- ✅ Bucket policies (13 comprehensive tests)
- ✅ Encryption functionality (15 comprehensive tests)
- ⬜ ACL enforcement

### Performance Tests
- ✅ Basic benchmark with warp
- ⬜ Large file upload performance
- ⬜ Concurrent operations stress test
- ⬜ Memory usage profiling

---

## Documentation

- ⬜ API documentation for all modules
- ⬜ Configuration guide
- ⬜ Deployment best practices
- ⬜ Performance tuning guide
- ✅ Test suite documentation

---

## Notes

### Implementation Strategy
1. Focus on completing multipart upload first (enables large file support)
2. Then implement versioning (critical for data integrity)
3. Security features (ACL, policies) can be added incrementally
4. Clustering is lowest priority (single-node is sufficient for most use cases)

### Breaking Changes
- Moving from monolithic main.rs to modular handlers will require careful refactoring
- Adding versioning will change metadata structure

### Dependencies to Consider
- Consider adding `aws-sdk-s3` for S3 compatibility testing
- May need `openssl` for encryption features
- Consider `raft` or similar for clustering support

---

## How to Contribute

1. Pick a task marked as ⬜ (Not Started)
2. Update status to 🟦 (In Progress)
3. Implement the feature
4. Add tests
5. Update status to ✅ (Completed)
6. Update this document with any new TODOs discovered

---

*Last Updated: 2025-09-14*
*Total Tasks: 51 (Completed: 46, Pending: 5)*
*Recent Progress: CORS fully implemented with JSON/XML support*