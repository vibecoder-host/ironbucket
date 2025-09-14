# IronBucket Documentation

Welcome to the IronBucket documentation! This guide will help you understand, deploy, and integrate IronBucket into your applications.

## 📚 Documentation Structure

### Getting Started
- **[Installation Guide](INSTALL.md)** - Complete installation instructions
  - Docker, Docker Compose, Binary, Source builds
  - Kubernetes and systemd setup
  - Post-installation configuration
- **[Configuration Guide](CONFIGURATION.md)** - All configuration options
  - Environment variables
  - Configuration files
  - Performance tuning
- [Quick Start](#quick-start)

### API Documentation
- **[API Reference](API.md)** - Complete HTTP API documentation
  - Bucket Operations
  - Object Operations
  - Multipart Upload
  - Advanced Features

### Usage Examples
- **[CLI Usage](USAGE_CLI.md)** - AWS CLI examples and commands
- **[Node.js Usage](USAGE_NODEJS.md)** - JavaScript/TypeScript integration
- **[Python Usage](USAGE_PYTHON.md)** - Python boto3 examples
- **[Rust Usage](USAGE_RUST.md)** - Rust aws-sdk-s3 examples

### Advanced Topics
- **[Performance Tuning](PERFORMANCE.md)** - Optimization guide
- **[Security](SECURITY.md)** - Authentication and encryption
- **[Troubleshooting](TROUBLESHOOTING.md)** - Common issues and solutions

---

### Development Setup

```bash
# Clone repo
git clone https://github.com/yourusername/ironbucket.git

# Install dependencies
cargo build

# Run tests
cargo test

# Run with debug logging
RUST_LOG=debug cargo run
```
