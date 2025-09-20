# IronBucket Encryption Guide

## Overview

IronBucket supports server-side encryption using AES-256-GCM algorithm with the ring cryptography library. This provides secure at-rest encryption for your S3-compatible storage.

## Features

- **AES-256-GCM encryption**: Industry-standard encryption algorithm
- **Ring library integration**: Cryptographically secure operations
- **Environment variable configuration**: Flexible deployment options
- **Bucket-level encryption policies**: Per-bucket encryption settings
- **Transparent encryption/decryption**: Seamless S3 API compatibility
- **Mixed object support**: Encrypted and unencrypted objects in same bucket

## Configuration

### Environment Variables

Configure encryption using these environment variables:

```bash
# Enable/disable global encryption (default: true in Docker)
ENABLE_ENCRYPTION=true

# Optional: Base64-encoded 256-bit encryption key
# If not provided, a key will be auto-generated
ENCRYPTION_KEY=<your-base64-key>

# Generate a key using:
# openssl rand -base64 32
```

### Docker Compose

Encryption is enabled by default in the Docker deployment:

```yaml
services:
  ironbucket:
    environment:
      - ENABLE_ENCRYPTION=true
      - ENCRYPTION_KEY=${ENCRYPTION_KEY:-}  # Optional master key
```

### .env File

Configure in `.env` file:

```bash
# Encryption Configuration
ENABLE_ENCRYPTION=true
ENCRYPTION_KEY=  # Leave empty for auto-generated key
```

## Usage

### Enable Bucket Encryption

```bash
# Create encryption configuration
cat > encryption.json << EOF
{
    "Rules": [
        {
            "ApplyServerSideEncryptionByDefault": {
                "SSEAlgorithm": "AES256"
            }
        }
    ]
}
EOF

# Apply to bucket
aws s3api put-bucket-encryption \
    --bucket my-bucket \
    --server-side-encryption-configuration file://encryption.json \
    --endpoint-url http://localhost:20000
```

### Check Bucket Encryption

```bash
aws s3api get-bucket-encryption \
    --bucket my-bucket \
    --endpoint-url http://localhost:20000
```

### Delete Bucket Encryption

```bash
aws s3api delete-bucket-encryption \
    --bucket my-bucket \
    --endpoint-url http://localhost:20000
```

## How It Works

1. **Global Encryption**: When `ENABLE_ENCRYPTION=true`, the encryption manager initializes
2. **Master Key**: Uses `ENCRYPTION_KEY` if provided, otherwise generates one
3. **Bucket Policy**: Check bucket-level encryption settings
4. **Object Upload**: If encryption enabled, generates per-object key and encrypts data
5. **Object Download**: Transparently decrypts data before returning
6. **Metadata Storage**: Encryption keys stored in object metadata

## Implementation Details

### Encryption Module (`src/encryption.rs`)

- **Algorithm**: AES-256-GCM with 96-bit nonces
- **Key Generation**: Ring's `SystemRandom` for cryptographically secure keys
- **Per-Object Keys**: Each object has unique encryption key
- **Nonce Generation**: Random 96-bit nonce for each encryption operation

### Storage

- Encrypted data stored in place of original
- Encryption metadata (key, nonce) stored in `.metadata` files
- Bucket encryption config persisted to `.encryption_config`

## Security Considerations

1. **Key Management**:
   - Master key should be securely stored
   - Consider using environment variable secrets management
   - Rotate keys periodically

2. **Access Control**:
   - Encryption complements but doesn't replace access control
   - Use IAM policies and bucket policies for authorization

3. **Transport Security**:
   - Always use HTTPS/TLS for data in transit
   - Encryption protects data at rest only

## Testing

Run encryption tests:

```bash
# Test against running Docker container
./tests/test-encryption-module-docker.sh

# Run all tests including encryption
./tests/run-all-tests.sh
```

## Performance

- Minimal overhead for encryption/decryption operations
- Ring library optimized for performance
- Suitable for objects up to 5GB (standard S3 limit)
- Tested with 5MB files showing negligible performance impact

## Troubleshooting

### Encryption Not Working

1. Check environment variables:
   ```bash
   docker exec ironbucket-ironbucket-1 env | grep ENCRYPT
   ```

2. Verify bucket encryption:
   ```bash
   aws s3api get-bucket-encryption --bucket my-bucket
   ```

3. Check logs:
   ```bash
   docker logs ironbucket-ironbucket-1
   ```

### Key Issues

- If `ENCRYPTION_KEY` is invalid base64, auto-generation will be used
- Keys must be exactly 32 bytes (256 bits) when decoded
- Use `openssl rand -base64 32` to generate valid keys

## API Compatibility

Fully compatible with S3 encryption APIs:
- `PUT Bucket encryption`
- `GET Bucket encryption`
- `DELETE Bucket encryption`
- Server-Side Encryption headers in responses

## Future Enhancements

- [ ] Key rotation support
- [ ] AWS KMS integration
- [ ] Customer-provided encryption keys (SSE-C)
- [ ] Encryption metrics and monitoring