# Installation Guide

This guide provides detailed instructions for installing and setting up IronBucket on various platforms.

## Table of Contents

- [Requirements](#requirements)
- [Quick Start](#quick-start)
- [Installation Methods](#installation-methods)
  - [Docker (Recommended)](#docker-recommended)
  - [Docker Compose](#docker-compose)
  - [From Source](#from-source)
  - [Binary Installation](#binary-installation)
  - [Kubernetes](#kubernetes)
- [Post-Installation](#post-installation)
- [Verification](#verification)
- [Troubleshooting](#troubleshooting)

## Requirements

### System Requirements

- **CPU**: 2+ cores recommended
- **RAM**: 4GB minimum, 8GB recommended
- **Storage**: 10GB minimum free space
- **OS**: Linux, macOS, Windows (with WSL2)

### Software Requirements

For Docker installation:
- Docker 20.10+
- Docker Compose 2.0+ (optional)

For building from source:
- Rust 1.70+
- Git
- OpenSSL development headers

## Quick Start

The fastest way to get IronBucket running:

```bash
# Using Docker
docker run -d \
  -p 9000:9000 \
  -v ./data:/data/s3 \
  -e ACCESS_KEY=admin \
  -e SECRET_KEY=admin \
  --name ironbucket \
  ironbucket:latest

# Verify it's running
curl http://localhost:9000
```

## Installation Methods

### Docker (Recommended)

#### Pull from Registry

```bash
# Pull the latest image
docker pull ironbucket/ironbucket:latest

# Run the container
docker run -d \
  --name ironbucket \
  -p 9000:9000 \
  -v $(pwd)/s3-data:/data/s3 \
  -e ACCESS_KEY=admin \
  -e SECRET_KEY=admin \
  -e RUST_LOG=ironbucket=info \
  ironbucket/ironbucket:latest
```

#### Build from Dockerfile

```bash
# Clone the repository
git clone https://github.com/yourusername/ironbucket.git
cd ironbucket

# Build the Docker image
docker build -t ironbucket:local .

# Run the container
docker run -d \
  --name ironbucket \
  -p 9000:9000 \
  -v $(pwd)/s3-data:/data/s3 \
  -e ACCESS_KEY=admin \
  -e SECRET_KEY=admin \
  ironbucket:local
```

### Docker Compose

#### Basic Setup

Create a `docker-compose.yml` file:

```yaml
version: '3.8'

services:
  ironbucket:
    image: ironbucket/ironbucket:latest
    container_name: ironbucket
    ports:
      - "9000:9000"
    volumes:
      - ./s3-data:/data/s3
    environment:
      - ACCESS_KEY=admin
      - SECRET_KEY=admin
      - RUST_LOG=ironbucket=info
      - STORAGE_PATH=/data/s3
    restart: unless-stopped
```

Start the service:

```bash
# Start in background
docker-compose up -d

# View logs
docker-compose logs -f ironbucket

# Stop the service
docker-compose down
```

#### Advanced Setup with Redis

```yaml
version: '3.8'

services:
  ironbucket:
    image: ironbucket/ironbucket:latest
    container_name: ironbucket
    ports:
      - "9000:9000"
    volumes:
      - ./s3-data:/data/s3
      - ./config:/etc/ironbucket
    environment:
      - ACCESS_KEY=admin
      - SECRET_KEY=admin
      - RUST_LOG=ironbucket=info
      - STORAGE_PATH=/data/s3
      - REDIS_URL=redis://redis:6379
    depends_on:
      - redis
    restart: unless-stopped

  redis:
    image: redis:7-alpine
    container_name: ironbucket-redis
    ports:
      - "6379:6379"
    volumes:
      - ./redis-data:/data
    restart: unless-stopped
```

### From Source

#### Prerequisites

Install Rust:

```bash
# Install Rust (Unix-like systems)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add to PATH
source $HOME/.cargo/env

# Verify installation
rustc --version
cargo --version
```

#### Build Steps

```bash
# Clone the repository
git clone https://github.com/yourusername/ironbucket.git
cd ironbucket

# Build in release mode
cargo build --release

# The binary will be at ./target/release/ironbucket
```

#### Run from Source

```bash
# Set environment variables
export ACCESS_KEY=admin
export SECRET_KEY=admin
export STORAGE_PATH=/tmp/s3-data
export RUST_LOG=ironbucket=info

# Run directly with cargo
cargo run --release

# Or run the built binary
./target/release/ironbucket
```

#### Install Globally

```bash
# Install to ~/.cargo/bin
cargo install --path .

# Now you can run from anywhere
ironbucket
```

### Binary Installation

#### Download Pre-built Binaries

1. Go to the [releases page](https://github.com/yourusername/ironbucket/releases)
2. Download the appropriate binary for your platform:
   - Linux x86_64: `ironbucket-linux-amd64`
   - Linux ARM64: `ironbucket-linux-arm64`
   - macOS x86_64: `ironbucket-darwin-amd64`
   - macOS ARM64: `ironbucket-darwin-arm64`
   - Windows: `ironbucket-windows-amd64.exe`

#### Linux/macOS Installation

```bash
# Download the binary (example for Linux x86_64)
wget https://github.com/yourusername/ironbucket/releases/download/v1.0.0/ironbucket-linux-amd64

# Make it executable
chmod +x ironbucket-linux-amd64

# Move to system path (optional)
sudo mv ironbucket-linux-amd64 /usr/local/bin/ironbucket

# Create data directory
mkdir -p /var/lib/ironbucket/data

# Run
ironbucket
```

#### Windows Installation

```powershell
# Download the binary
Invoke-WebRequest -Uri "https://github.com/yourusername/ironbucket/releases/download/v1.0.0/ironbucket-windows-amd64.exe" -OutFile "ironbucket.exe"

# Create data directory
New-Item -ItemType Directory -Force -Path "C:\ironbucket\data"

# Run
.\ironbucket.exe
```

### Kubernetes

#### Using Helm

```bash
# Add the IronBucket Helm repository
helm repo add ironbucket https://charts.ironbucket.io
helm repo update

# Install with default values
helm install my-ironbucket ironbucket/ironbucket

# Install with custom values
helm install my-ironbucket ironbucket/ironbucket \
  --set persistence.size=100Gi \
  --set resources.requests.memory=4Gi \
  --set credentials.accessKey=myaccesskey \
  --set credentials.secretKey=mysecretkey
```

#### Using kubectl

Create a deployment file `ironbucket-deployment.yaml`:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: ironbucket
spec:
  replicas: 1
  selector:
    matchLabels:
      app: ironbucket
  template:
    metadata:
      labels:
        app: ironbucket
    spec:
      containers:
      - name: ironbucket
        image: ironbucket/ironbucket:latest
        ports:
        - containerPort: 9000
        env:
        - name: ACCESS_KEY
          valueFrom:
            secretKeyRef:
              name: ironbucket-secrets
              key: access-key
        - name: SECRET_KEY
          valueFrom:
            secretKeyRef:
              name: ironbucket-secrets
              key: secret-key
        - name: STORAGE_PATH
          value: /data/s3
        volumeMounts:
        - name: storage
          mountPath: /data/s3
      volumes:
      - name: storage
        persistentVolumeClaim:
          claimName: ironbucket-pvc

---
apiVersion: v1
kind: Service
metadata:
  name: ironbucket
spec:
  selector:
    app: ironbucket
  ports:
    - protocol: TCP
      port: 9000
      targetPort: 9000
  type: LoadBalancer

---
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: ironbucket-pvc
spec:
  accessModes:
    - ReadWriteOnce
  resources:
    requests:
      storage: 100Gi

---
apiVersion: v1
kind: Secret
metadata:
  name: ironbucket-secrets
type: Opaque
data:
  access-key: bWluaW9hZG1pbg==  # base64 encoded
  secret-key: bWluaW9hZG1pbg==  # base64 encoded
```

Deploy:

```bash
# Apply the configuration
kubectl apply -f ironbucket-deployment.yaml

# Check status
kubectl get pods -l app=ironbucket
kubectl get svc ironbucket

# View logs
kubectl logs -l app=ironbucket -f
```

## Post-Installation

### Create Systemd Service (Linux)

Create `/etc/systemd/system/ironbucket.service`:

```ini
[Unit]
Description=IronBucket S3-compatible storage server
After=network.target

[Service]
Type=simple
User=ironbucket
Group=ironbucket
WorkingDirectory=/opt/ironbucket
ExecStart=/usr/local/bin/ironbucket
Restart=always
RestartSec=10

# Environment variables
Environment="ACCESS_KEY=admin"
Environment="SECRET_KEY=admin"
Environment="STORAGE_PATH=/var/lib/ironbucket/data"
Environment="RUST_LOG=ironbucket=info"

# Security
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/ironbucket

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
# Create user and directories
sudo useradd -r -s /bin/false ironbucket
sudo mkdir -p /var/lib/ironbucket/data
sudo chown -R ironbucket:ironbucket /var/lib/ironbucket

# Enable and start service
sudo systemctl daemon-reload
sudo systemctl enable ironbucket
sudo systemctl start ironbucket

# Check status
sudo systemctl status ironbucket
```

### Configure Firewall

#### iptables

```bash
# Allow port 9000
sudo iptables -A INPUT -p tcp --dport 9000 -j ACCEPT
sudo iptables-save > /etc/iptables/rules.v4
```

#### firewalld

```bash
# Add port permanently
sudo firewall-cmd --permanent --add-port=9000/tcp
sudo firewall-cmd --reload
```

#### ufw

```bash
# Allow port
sudo ufw allow 9000/tcp
sudo ufw reload
```

### Setup SSL/TLS

Using a reverse proxy (nginx):

```nginx
server {
    listen 443 ssl http2;
    server_name s3.example.com;

    ssl_certificate /etc/ssl/certs/s3.example.com.crt;
    ssl_certificate_key /etc/ssl/private/s3.example.com.key;

    client_max_body_size 5G;

    location / {
        proxy_pass http://localhost:9000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # For large file uploads
        proxy_request_buffering off;
        proxy_buffering off;
        proxy_http_version 1.1;
    }
}
```

## Verification

### Basic Health Check

```bash
# Check if server is responding
curl -I http://localhost:9000

# Expected response:
# HTTP/1.1 200 OK
# Server: IronBucket
```

### Test with AWS CLI

```bash
# Configure AWS CLI
export AWS_ACCESS_KEY_ID=admin
export AWS_SECRET_ACCESS_KEY=admin
export AWS_ENDPOINT=http://localhost:9000

# Create a test bucket
aws --endpoint-url $AWS_ENDPOINT s3 mb s3://test-bucket

# Upload a test file
echo "Hello, IronBucket!" > test.txt
aws --endpoint-url $AWS_ENDPOINT s3 cp test.txt s3://test-bucket/

# List buckets
aws --endpoint-url $AWS_ENDPOINT s3 ls

# List objects
aws --endpoint-url $AWS_ENDPOINT s3 ls s3://test-bucket/

# Download the file
aws --endpoint-url $AWS_ENDPOINT s3 cp s3://test-bucket/test.txt downloaded.txt

# Verify content
cat downloaded.txt

# Clean up
aws --endpoint-url $AWS_ENDPOINT s3 rm s3://test-bucket/test.txt
aws --endpoint-url $AWS_ENDPOINT s3 rb s3://test-bucket
```

### Test with curl

```bash
# List buckets (requires signature, will fail without auth)
curl -X GET http://localhost:9000/

# Get server info
curl -I http://localhost:9000/
```

## Troubleshooting

### Common Issues

#### Port Already in Use

```bash
# Check what's using port 9000
lsof -i :9000
# or
netstat -tlnp | grep 9000

# Kill the process or change IronBucket port
export PORT=9001
```

#### Permission Denied

```bash
# Fix storage directory permissions
sudo chown -R $(whoami):$(whoami) ./s3-data
chmod -R 755 ./s3-data
```

#### Container Won't Start

```bash
# Check logs
docker logs ironbucket

# Common fixes:
# 1. Ensure data directory exists
mkdir -p ./s3-data

# 2. Check environment variables
docker run --rm ironbucket env

# 3. Try with minimal config
docker run --rm -p 9000:9000 ironbucket
```

#### Build Failures

```bash
# Update Rust
rustup update

# Clean build
cargo clean
cargo build --release

# Check for missing dependencies
# On Ubuntu/Debian:
sudo apt-get install pkg-config libssl-dev

# On RHEL/CentOS/Fedora:
sudo yum install pkgconfig openssl-devel

# On macOS:
brew install openssl
```

### Performance Issues

```bash
# Increase file descriptor limits
ulimit -n 65536

# For systemd services, add to service file:
LimitNOFILE=65536

# Check current limits
ulimit -a
```

### Debug Mode

```bash
# Enable debug logging
export RUST_LOG=ironbucket=debug,tower_http=debug

# Run with backtrace
export RUST_BACKTRACE=1
./ironbucket
```

## Uninstallation

### Docker

```bash
# Stop and remove container
docker stop ironbucket
docker rm ironbucket

# Remove image
docker rmi ironbucket/ironbucket:latest

# Remove data (optional)
rm -rf ./s3-data
```

### Systemd Service

```bash
# Stop and disable service
sudo systemctl stop ironbucket
sudo systemctl disable ironbucket

# Remove service file
sudo rm /etc/systemd/system/ironbucket.service
sudo systemctl daemon-reload

# Remove binary
sudo rm /usr/local/bin/ironbucket

# Remove data (optional)
sudo rm -rf /var/lib/ironbucket
```

### Kubernetes

```bash
# Using Helm
helm uninstall my-ironbucket

# Using kubectl
kubectl delete -f ironbucket-deployment.yaml

# Remove PVC (this will delete data)
kubectl delete pvc ironbucket-pvc
```

## Next Steps

- [Configuration Guide](./CONFIGURATION.md) - Detailed configuration options
- [API Documentation](./API.md) - Complete API reference
- [Usage Examples](./README.md#usage-examples) - Client library examples
- [Performance Tuning](./PERFORMANCE.md) - Optimization guide