# Troubleshooting Guide

Comprehensive troubleshooting guide for common IronBucket issues and their solutions.

## Table of Contents

- [Quick Diagnostics](#quick-diagnostics)
- [Installation Issues](#installation-issues)
- [Startup Problems](#startup-problems)
- [Connection Issues](#connection-issues)
- [Authentication Errors](#authentication-errors)
- [Performance Problems](#performance-problems)
- [Storage Issues](#storage-issues)
- [Upload/Download Failures](#uploaddownload-failures)
- [Docker Issues](#docker-issues)
- [Memory Issues](#memory-issues)
- [Logging & Debugging](#logging--debugging)
- [Common Error Messages](#common-error-messages)
- [Recovery Procedures](#recovery-procedures)

## Quick Diagnostics

### Health Check Script

```bash
#!/bin/bash
# ironbucket_health_check.sh - Comprehensive health check

echo "=== IronBucket Health Check ==="
echo

# 1. Check if IronBucket is running
echo "1. Process Status:"
if pgrep ironbucket > /dev/null; then
    echo "   ✓ IronBucket is running (PID: $(pgrep ironbucket))"
else
    echo "   ✗ IronBucket is not running"
fi

# 2. Check port availability
echo "2. Port Status:"
if netstat -tuln | grep -q ":9000 "; then
    echo "   ✓ Port 9000 is listening"
else
    echo "   ✗ Port 9000 is not listening"
fi

# 3. Test HTTP connectivity
echo "3. HTTP Connectivity:"
if curl -s -o /dev/null -w "%{http_code}" http://localhost:9000 | grep -q "200\|403"; then
    echo "   ✓ HTTP endpoint responsive"
else
    echo "   ✗ HTTP endpoint not responding"
fi

# 4. Check disk space
echo "4. Disk Space:"
DISK_USAGE=$(df -h /data/s3 2>/dev/null | awk 'NR==2 {print $5}' | sed 's/%//')
if [[ -n "$DISK_USAGE" && "$DISK_USAGE" -lt 90 ]]; then
    echo "   ✓ Disk usage: ${DISK_USAGE}%"
else
    echo "   ✗ Disk usage critical or path not found"
fi

# 5. Check memory
echo "5. Memory Usage:"
MEM_USAGE=$(free | awk 'NR==2 {printf "%.1f", $3*100/$2}')
echo "   Memory usage: ${MEM_USAGE}%"

# 6. Check logs for errors
echo "6. Recent Errors:"
if [[ -f /var/log/ironbucket/server.log ]]; then
    ERROR_COUNT=$(tail -n 1000 /var/log/ironbucket/server.log | grep -c ERROR)
    echo "   Errors in last 1000 lines: $ERROR_COUNT"
else
    echo "   ✗ Log file not found"
fi

# 7. Test S3 operations
echo "7. S3 Operations Test:"
export AWS_ACCESS_KEY_ID="${IRONBUCKET_ACCESS_KEY}"
export AWS_SECRET_ACCESS_KEY="${IRONBUCKET_SECRET_KEY}"

if aws --endpoint-url http://localhost:9000 s3 ls &>/dev/null; then
    echo "   ✓ S3 API operational"
else
    echo "   ✗ S3 API not working"
fi
```

### Quick Fix Script

```bash
#!/bin/bash
# quick_fix.sh - Attempt common fixes

echo "Attempting common fixes..."

# 1. Clear cache
echo "Clearing cache..."
rm -rf /tmp/ironbucket/*

# 2. Fix permissions
echo "Fixing permissions..."
chown -R ironbucket:ironbucket /data/s3
chmod -R 755 /data/s3

# 3. Restart service
echo "Restarting IronBucket..."
systemctl restart ironbucket

# 4. Check status
sleep 3
systemctl status ironbucket
```

## Installation Issues

### Problem: Rust compilation fails

**Symptoms:**
```
error: failed to compile ironbucket
```

**Solutions:**

1. **Update Rust:**
```bash
rustup update
rustup default stable
```

2. **Install missing dependencies:**
```bash
# Ubuntu/Debian
apt-get update
apt-get install -y pkg-config libssl-dev build-essential

# RHEL/CentOS/Fedora
yum install -y pkgconfig openssl-devel gcc

# macOS
brew install openssl pkg-config
```

3. **Clean and rebuild:**
```bash
cargo clean
cargo build --release
```

### Problem: Docker build fails

**Symptoms:**
```
docker build error: failed to solve with frontend dockerfile.v0
```

**Solutions:**

1. **Update Docker:**
```bash
# Update Docker
apt-get update && apt-get upgrade docker-ce

# Or reinstall
curl -fsSL https://get.docker.com | sh
```

2. **Clear Docker cache:**
```bash
docker system prune -af
docker builder prune -af
```

3. **Build with no cache:**
```bash
docker build --no-cache -t ironbucket .
```

## Startup Problems

### Problem: IronBucket won't start

**Symptoms:**
- Service fails to start
- No process running
- Exit code 1

**Solutions:**

1. **Check logs:**
```bash
# Systemd logs
journalctl -u ironbucket -n 50

# Docker logs
docker logs ironbucket

# Direct logs
tail -f /var/log/ironbucket/server.log
```

2. **Verify configuration:**
```bash
# Check environment variables
printenv | grep IRONBUCKET

# Validate config file
ironbucket --validate-config

# Test with minimal config
RUST_LOG=debug ironbucket --test
```

3. **Check port conflicts:**
```bash
# Find what's using port 9000
lsof -i :9000
netstat -tlnp | grep 9000

# Kill conflicting process
kill -9 $(lsof -t -i:9000)

# Or change port
export PORT=9001
```

### Problem: Permission denied errors

**Symptoms:**
```
Error: Permission denied (os error 13)
```

**Solutions:**

```bash
# Fix ownership
chown -R ironbucket:ironbucket /opt/ironbucket
chown -R ironbucket:ironbucket /data/s3

# Fix permissions
chmod 755 /opt/ironbucket
chmod -R 755 /data/s3

# For systemd service
chmod 644 /etc/systemd/system/ironbucket.service

# SELinux context (if applicable)
semanage fcontext -a -t httpd_sys_content_t "/data/s3(/.*)?"
restorecon -Rv /data/s3
```

## Connection Issues

### Problem: Cannot connect to IronBucket

**Symptoms:**
- Connection refused
- Timeout errors
- "No route to host"

**Solutions:**

1. **Check if service is running:**
```bash
ps aux | grep ironbucket
systemctl status ironbucket
docker ps | grep ironbucket
```

2. **Test local connectivity:**
```bash
# Test localhost
curl -v http://localhost:9000

# Test with telnet
telnet localhost 9000

# Test with nc
nc -zv localhost 9000
```

3. **Check firewall:**
```bash
# iptables
iptables -L -n | grep 9000

# firewalld
firewall-cmd --list-ports

# ufw
ufw status

# Allow port
iptables -A INPUT -p tcp --dport 9000 -j ACCEPT
# or
firewall-cmd --permanent --add-port=9000/tcp && firewall-cmd --reload
# or
ufw allow 9000
```

4. **Check binding address:**
```bash
# Ensure it's not bound to localhost only
netstat -tlnp | grep 9000

# Should show 0.0.0.0:9000 not 127.0.0.1:9000
# Fix:
export HOST=0.0.0.0
```

### Problem: SSL/TLS connection errors

**Symptoms:**
```
SSL certificate problem: self signed certificate
curl: (60) SSL certificate verification failed
```

**Solutions:**

1. **For self-signed certificates:**
```bash
# Disable SSL verification (testing only)
export AWS_CLI_VERIFY_SSL=false
curl -k https://localhost:9000

# Add certificate to trust store
cp /path/to/cert.pem /usr/local/share/ca-certificates/
update-ca-certificates
```

2. **Generate proper certificates:**
```bash
# Using Let's Encrypt
certbot certonly --standalone -d s3.example.com

# Update IronBucket config
export TLS_CERT=/etc/letsencrypt/live/s3.example.com/fullchain.pem
export TLS_KEY=/etc/letsencrypt/live/s3.example.com/privkey.pem
```

## Authentication Errors

### Problem: Access Denied / Invalid Credentials

**Symptoms:**
```
AccessDenied: Access Denied
InvalidAccessKeyId: The AWS Access Key Id you provided does not exist
SignatureDoesNotMatch: The request signature we calculated does not match
```

**Solutions:**

1. **Verify credentials:**
```bash
# Check environment variables
echo $IRONBUCKET_ACCESS_KEY
echo $IRONBUCKET_SECRET_KEY

# Check AWS CLI config
cat ~/.aws/credentials

# Test with explicit credentials
aws --endpoint-url http://localhost:9000 \
    --access-key YOUR_KEY \
    --secret-key YOUR_SECRET \
    s3 ls
```

2. **Fix time synchronization:**
```bash
# Check system time
date

# Sync time
ntpdate -s time.nist.gov
# or
timedatectl set-ntp true
```

3. **Reset credentials:**
```bash
# Generate new credentials
export ACCESS_KEY=$(openssl rand -hex 16)
export SECRET_KEY=$(openssl rand -base64 32)

# Update configuration
echo "ACCESS_KEY=$ACCESS_KEY" >> /etc/ironbucket/.env
echo "SECRET_KEY=$SECRET_KEY" >> /etc/ironbucket/.env

# Restart service
systemctl restart ironbucket
```

## Performance Problems

### Problem: Slow upload/download speeds

**Symptoms:**
- Transfer speeds below expected
- High latency
- Timeouts on large files

**Solutions:**

1. **Check network:**
```bash
# Test network speed
iperf3 -c localhost -p 9000

# Check MTU
ip link show | grep mtu

# Optimize MTU
ip link set dev eth0 mtu 9000
```

2. **Optimize IronBucket:**
```bash
# Increase workers
export WORKERS=16

# Increase buffer sizes
export RECV_BUFFER_SIZE=1048576
export SEND_BUFFER_SIZE=1048576

# Enable caching
export ENABLE_CACHE=true
export REDIS_URL=redis://localhost:6379
```

3. **Check disk I/O:**
```bash
# Test disk speed
dd if=/dev/zero of=/data/s3/test bs=1M count=1000

# Check I/O stats
iostat -x 1

# Use faster storage
mount -t tmpfs -o size=10G tmpfs /data/s3/temp
```

### Problem: High CPU usage

**Symptoms:**
- CPU at 100%
- System becomes unresponsive
- Slow response times

**Solutions:**

1. **Profile the application:**
```bash
# Use perf
perf top -p $(pgrep ironbucket)

# Generate flame graph
perf record -F 99 -p $(pgrep ironbucket) -g -- sleep 30
perf script | flamegraph.pl > flame.svg
```

2. **Limit resources:**
```bash
# CPU limit with systemd
[Service]
CPUQuota=80%

# With Docker
docker update --cpus="2.0" ironbucket
```

3. **Optimize configuration:**
```bash
# Reduce logging
export RUST_LOG=ironbucket=warn

# Disable unnecessary features
export ENABLE_METRICS=false
export ENABLE_AUDIT_LOG=false
```

## Storage Issues

### Problem: Disk full

**Symptoms:**
```
No space left on device
Cannot write to storage
```

**Solutions:**

1. **Clean up space:**
```bash
# Check disk usage
df -h
du -sh /data/s3/*

# Find large files
find /data/s3 -type f -size +1G -exec ls -lh {} \;

# Clean old logs
find /var/log -name "*.log" -mtime +30 -delete

# Clean temp files
rm -rf /tmp/ironbucket/*
```

2. **Implement lifecycle policies:**
```bash
# Create lifecycle policy
cat > lifecycle.json <<EOF
{
  "Rules": [{
    "Status": "Enabled",
    "Expiration": {
      "Days": 30
    },
    "ID": "DeleteOldObjects"
  }]
}
EOF

aws s3api put-bucket-lifecycle-configuration \
  --bucket my-bucket \
  --lifecycle-configuration file://lifecycle.json
```

### Problem: Corrupted storage

**Symptoms:**
- Objects cannot be retrieved
- Checksum mismatches
- Metadata errors

**Solutions:**

1. **Verify and repair:**
```bash
# Check filesystem
fsck -y /dev/sda1

# Verify object integrity
find /data/s3 -type f -exec md5sum {} \; > checksums.txt

# Rebuild metadata
ironbucket --rebuild-metadata
```

2. **Restore from backup:**
```bash
# Stop service
systemctl stop ironbucket

# Restore data
rsync -av /backup/s3/ /data/s3/

# Start service
systemctl start ironbucket
```

## Upload/Download Failures

### Problem: Multipart upload fails

**Symptoms:**
```
NoSuchUpload: The specified upload does not exist
EntityTooLarge: Your proposed upload exceeds the maximum allowed size
```

**Solutions:**

1. **Check configuration:**
```bash
# Increase limits
export MAX_FILE_SIZE=53687091200  # 50GB
export MULTIPART_CHUNK_SIZE=10485760  # 10MB

# Check temp space
df -h /tmp
```

2. **Clean incomplete uploads:**
```bash
# List incomplete uploads
aws s3api list-multipart-uploads --bucket my-bucket

# Abort stuck upload
aws s3api abort-multipart-upload \
  --bucket my-bucket \
  --key my-object \
  --upload-id UPLOAD_ID
```

### Problem: Timeout errors

**Symptoms:**
```
RequestTimeout: Your socket connection to the server was not read from or written to
```

**Solutions:**

```bash
# Increase timeouts
export REQUEST_TIMEOUT=600
export IDLE_TIMEOUT=120

# For AWS CLI
aws configure set cli_read_timeout 600
aws configure set cli_connect_timeout 60

# For applications
# Increase client timeout settings
```

## Docker Issues

### Problem: Container keeps restarting

**Symptoms:**
- Container in restart loop
- Exit code 125, 126, or 127

**Solutions:**

1. **Check logs:**
```bash
docker logs --tail 50 ironbucket
docker inspect ironbucket
```

2. **Fix common issues:**
```bash
# Permission issues
docker exec ironbucket chmod -R 755 /data/s3

# Resource limits
docker update --memory="2g" --memory-swap="4g" ironbucket

# Recreate container
docker stop ironbucket
docker rm ironbucket
docker run -d --name ironbucket ...
```

### Problem: Cannot access from host

**Symptoms:**
- Works inside container but not from host
- Connection refused from host

**Solutions:**

```bash
# Check port mapping
docker port ironbucket

# Correct port mapping
docker run -p 9000:9000 ironbucket  # Not just -p 9000

# Check network
docker network ls
docker network inspect bridge

# Use host network (Linux only)
docker run --network host ironbucket
```

## Memory Issues

### Problem: Out of memory errors

**Symptoms:**
```
memory allocation failed
Out of memory: Kill process
```

**Solutions:**

1. **Increase memory limits:**
```bash
# System limits
ulimit -v unlimited

# Systemd limits
[Service]
MemoryLimit=4G

# Docker limits
docker run -m 4g ironbucket
```

2. **Optimize memory usage:**
```bash
# Reduce cache size
export MAX_MEMORY_CACHE=536870912  # 512MB

# Reduce buffer pool
export BUFFER_POOL_SIZE=50

# Reduce workers
export WORKERS=4
```

3. **Add swap (temporary fix):**
```bash
# Create swap file
fallocate -l 4G /swapfile
chmod 600 /swapfile
mkswap /swapfile
swapon /swapfile

# Make permanent
echo '/swapfile none swap sw 0 0' >> /etc/fstab
```

## Logging & Debugging

### Enable debug logging

```bash
# Maximum verbosity
export RUST_LOG=trace
export RUST_BACKTRACE=full

# Specific modules
export RUST_LOG=ironbucket::storage=debug,ironbucket::auth=trace

# With timestamps
export RUST_LOG_STYLE=always
```

### Analyze logs

```bash
# Common error patterns
grep -E "ERROR|WARN|PANIC" /var/log/ironbucket/server.log

# Request tracking
grep "request_id=12345" /var/log/ironbucket/server.log

# Performance analysis
awk '/request_time/ {sum+=$NF; count++} END {print "Avg:", sum/count}' server.log
```

### Debug tools

```bash
# Trace system calls
strace -p $(pgrep ironbucket)

# Monitor file operations
lsof -p $(pgrep ironbucket)

# Network debugging
tcpdump -i any -n port 9000

# Memory debugging
valgrind --leak-check=full ironbucket
```

## Common Error Messages

### "Address already in use"

```bash
# Find and kill process
lsof -i :9000
kill -9 $(lsof -t -i:9000)

# Or change port
export PORT=9001
```

### "Too many open files"

```bash
# Increase limit
ulimit -n 65536

# Permanent fix
echo "* soft nofile 65536" >> /etc/security/limits.conf
echo "* hard nofile 65536" >> /etc/security/limits.conf
```

### "Connection pool timeout"

```bash
# Increase pool size
export REDIS_POOL_SIZE=100

# Increase timeout
export REDIS_CONNECTION_TIMEOUT=30
```

### "Invalid bucket name"

```bash
# Bucket naming rules:
# - 3-63 characters
# - Lowercase letters, numbers, hyphens
# - Must start and end with letter or number
# - No consecutive hyphens
# - Not formatted as IP address

# Valid: my-bucket-123
# Invalid: My_Bucket, bucket., 192.168.1.1
```

## Recovery Procedures

### Emergency recovery

```bash
#!/bin/bash
# emergency_recovery.sh

echo "Starting emergency recovery..."

# 1. Stop service
systemctl stop ironbucket

# 2. Backup current state
tar -czf /backup/ironbucket-$(date +%Y%m%d-%H%M%S).tar.gz /data/s3

# 3. Check and repair filesystem
fsck -y /dev/sda1

# 4. Reset permissions
chown -R ironbucket:ironbucket /data/s3
chmod -R 755 /data/s3

# 5. Clear cache and temp
rm -rf /tmp/ironbucket/*
rm -rf /var/cache/ironbucket/*

# 6. Validate configuration
ironbucket --validate-config

# 7. Start in safe mode
RUST_LOG=debug WORKERS=1 ironbucket --safe-mode

# 8. Test basic operations
aws --endpoint-url http://localhost:9000 s3 ls

# 9. If successful, restart normally
systemctl start ironbucket

echo "Recovery complete"
```

### Data recovery

```bash
#!/bin/bash
# data_recovery.sh

# Recover deleted objects from filesystem
find /data/s3/.trash -type f -mtime -7 -exec cp {} /data/s3/recovered/ \;

# Recover from backup
rsync -av --progress /backup/s3/ /data/s3/

# Verify integrity
find /data/s3 -type f -exec md5sum {} \; | md5sum -c -

# Rebuild metadata
ironbucket --rebuild-metadata --verify
```

## Getting Help

### Collect diagnostic information

```bash
#!/bin/bash
# collect_diagnostics.sh

DIAG_DIR="/tmp/ironbucket-diag-$(date +%Y%m%d-%H%M%S)"
mkdir -p $DIAG_DIR

# System info
uname -a > $DIAG_DIR/system.txt
free -h >> $DIAG_DIR/system.txt
df -h >> $DIAG_DIR/system.txt

# IronBucket info
ironbucket --version > $DIAG_DIR/version.txt
printenv | grep IRONBUCKET > $DIAG_DIR/env.txt

# Logs
tail -n 1000 /var/log/ironbucket/server.log > $DIAG_DIR/server.log
journalctl -u ironbucket -n 1000 > $DIAG_DIR/systemd.log

# Configuration
cp /etc/ironbucket/config.toml $DIAG_DIR/ 2>/dev/null

# Network
netstat -tlnp > $DIAG_DIR/network.txt
iptables -L -n > $DIAG_DIR/firewall.txt

# Create archive
tar -czf ironbucket-diagnostics.tar.gz -C /tmp $(basename $DIAG_DIR)

echo "Diagnostics collected: ironbucket-diagnostics.tar.gz"
```

### Support channels

1. **GitHub Issues**: Report bugs with diagnostic information
2. **Community Forum**: Ask questions and share solutions
3. **Documentation**: Check latest docs for updates
4. **Emergency Support**: For critical production issues

## Next Steps

- [Performance Guide](./PERFORMANCE.md) - Optimize after fixing issues
- [Security Guide](./SECURITY.md) - Secure your deployment
- [Configuration Guide](./CONFIGURATION.md) - Fine-tune settings