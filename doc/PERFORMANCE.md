# Performance Tuning Guide

This guide provides comprehensive performance optimization strategies for IronBucket deployments.

## Table of Contents

- [Benchmarking](#benchmarking)
- [System Optimization](#system-optimization)
- [IronBucket Configuration](#ironbucket-configuration)
- [Network Optimization](#network-optimization)
- [Storage Optimization](#storage-optimization)
- [Caching Strategy](#caching-strategy)
- [Monitoring & Profiling](#monitoring--profiling)
- [Load Testing](#load-testing)
- [Performance Targets](#performance-targets)
- [Troubleshooting Performance Issues](#troubleshooting-performance-issues)

## Benchmarking

### Current Performance Metrics

On standard hardware (8 cores, 16GB RAM), IronBucket achieves:

| Operation | Throughput | Latency (p50) | Latency (p99) |
|-----------|------------|---------------|---------------|
| PUT | 3,341 ops/sec | 2ms | 5ms |
| GET | 10,026 ops/sec | 1ms | 3ms |
| DELETE | 2,227 ops/sec | 2ms | 4ms |
| HEAD | 6,684 ops/sec | 1ms | 2ms |
| LIST | 4,521 ops/sec | 3ms | 8ms |

### Benchmarking Tools

#### MinIO Warp

```bash
# Install warp
wget https://github.com/minio/warp/releases/latest/download/warp_Linux_x86_64.tar.gz
tar -xzf warp_Linux_x86_64.tar.gz

# Run mixed workload benchmark
./warp mixed \
  --host=172.17.0.1:20000 \
  --access-key=$IRONBUCKET_ACCESS_KEY \
  --secret-key=$IRONBUCKET_SECRET_KEY \
  --objects=10000 \
  --duration=60s \
  --concurrent=50

# Specific operation benchmarks
./warp get --host=172.17.0.1:20000 --duration=60s --concurrent=100
./warp put --host=172.17.0.1:20000 --obj.size=1MB --duration=60s
./warp delete --host=172.17.0.1:20000 --duration=60s --batch=100
```

#### Custom Benchmark Script

```bash
#!/bin/bash
# benchmark.sh - Custom IronBucket benchmark

ENDPOINT="http://172.17.0.1:20000"
BUCKET="benchmark-bucket"
THREADS=50
DURATION=60

# Create test bucket
aws --endpoint-url $ENDPOINT s3 mb s3://$BUCKET

# Generate test data
for i in {1..1000}; do
    dd if=/dev/urandom of=test-$i.dat bs=1M count=1 2>/dev/null
done

# Run concurrent uploads
echo "Starting upload benchmark..."
time parallel -j $THREADS \
    "aws --endpoint-url $ENDPOINT s3 cp test-{}.dat s3://$BUCKET/" \
    ::: {1..1000}

# Run concurrent downloads
echo "Starting download benchmark..."
time parallel -j $THREADS \
    "aws --endpoint-url $ENDPOINT s3 cp s3://$BUCKET/test-{}.dat /tmp/" \
    ::: {1..1000}

# Cleanup
aws --endpoint-url $ENDPOINT s3 rb s3://$BUCKET --force
rm test-*.dat
```

## System Optimization

### Operating System Tuning

#### Linux Kernel Parameters

Add to `/etc/sysctl.conf`:

```bash
# Network optimizations
net.core.somaxconn = 65535
net.core.netdev_max_backlog = 65536
net.ipv4.tcp_max_syn_backlog = 65536
net.ipv4.tcp_fin_timeout = 10
net.ipv4.tcp_tw_reuse = 1
net.ipv4.tcp_keepalive_time = 60
net.ipv4.tcp_keepalive_intvl = 10
net.ipv4.tcp_keepalive_probes = 6
net.ipv4.ip_local_port_range = 1024 65535

# File system
fs.file-max = 2097152
fs.nr_open = 2097152

# Memory management
vm.swappiness = 10
vm.dirty_ratio = 15
vm.dirty_background_ratio = 5
vm.max_map_count = 262144

# Apply settings
sysctl -p
```

#### File Descriptor Limits

Edit `/etc/security/limits.conf`:

```bash
* soft nofile 1048576
* hard nofile 1048576
* soft nproc 65536
* hard nproc 65536
```

For systemd services, add to service file:

```ini
[Service]
LimitNOFILE=1048576
LimitNPROC=65536
```

### CPU Optimization

#### CPU Governor

```bash
# Check current governor
cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Set to performance mode
for cpu in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do
    echo performance > $cpu
done

# Make permanent
apt-get install cpufrequtils
echo 'GOVERNOR="performance"' > /etc/default/cpufrequtils
```

#### CPU Affinity

```bash
# Bind IronBucket to specific CPUs
taskset -c 0-7 ./ironbucket

# Or use systemd
[Service]
CPUAffinity=0-7
```

### Memory Optimization

#### Huge Pages

```bash
# Enable transparent huge pages
echo always > /sys/kernel/mm/transparent_hugepage/enabled
echo always > /sys/kernel/mm/transparent_hugepage/defrag

# Configure huge pages
echo 1024 > /proc/sys/vm/nr_hugepages
```

#### NUMA Optimization

```bash
# Check NUMA topology
numactl --hardware

# Run with NUMA binding
numactl --cpunodebind=0 --membind=0 ./ironbucket
```

## IronBucket Configuration

### Optimal Environment Variables

```bash
# Core settings
export WORKERS=0  # Auto-detect CPU cores
export ASYNC_THREADS=16
export BLOCKING_THREADS=512

# Network settings
export MAX_CONNECTIONS=10000
export KEEP_ALIVE_TIMEOUT=300
export TCP_NODELAY=true
export TCP_KEEPALIVE=7200

# Buffer sizes
export RECV_BUFFER_SIZE=524288  # 512KB
export SEND_BUFFER_SIZE=524288  # 512KB
export BUFFER_POOL_SIZE=200

# Request handling
export MAX_REQUEST_SIZE=10737418240  # 10GB
export REQUEST_TIMEOUT=600  # 10 minutes
export IDLE_TIMEOUT=120

# Storage settings
export MAX_FILE_SIZE=53687091200  # 50GB
export MULTIPART_CHUNK_SIZE=10485760  # 10MB
export ENABLE_COMPRESSION=false  # Disable for speed

# Cache settings
export ENABLE_CACHE=true
export MAX_MEMORY_CACHE=4294967296  # 4GB
export DIR_CACHE_SIZE=10000
export DIR_CACHE_TTL=300

# Logging (reduce for performance)
export RUST_LOG=ironbucket=warn
```

## Network Optimization

### Network Interface Tuning

```bash
# Increase network interface queue size
ip link set dev eth0 txqueuelen 10000

# Enable offloading features
ethtool -K eth0 gso on
ethtool -K eth0 gro on
ethtool -K eth0 tso on

# Increase ring buffer sizes
ethtool -G eth0 rx 4096 tx 4096
```

### TCP Optimization

```bash
# TCP congestion control
echo bbr > /proc/sys/net/ipv4/tcp_congestion_control

# Enable TCP Fast Open
echo 3 > /proc/sys/net/ipv4/tcp_fastopen

# Increase TCP buffers
echo "4096 87380 67108864" > /proc/sys/net/ipv4/tcp_rmem
echo "4096 65536 67108864" > /proc/sys/net/ipv4/tcp_wmem
```

### Load Balancing

#### HAProxy Configuration

```haproxy
global
    maxconn 100000
    tune.ssl.default-dh-param 2048
    nbproc 4
    cpu-map 1 0
    cpu-map 2 1
    cpu-map 3 2
    cpu-map 4 3

defaults
    mode http
    timeout connect 5s
    timeout client 300s
    timeout server 300s
    option http-keep-alive
    option forwardfor

backend ironbucket_cluster
    balance leastconn
    option httpchk GET /health
    server ironbucket1 172.17.0.1:9000 check maxconn 5000
    server ironbucket2 172.17.0.2:9000 check maxconn 5000
    server ironbucket3 172.17.0.3:9000 check maxconn 5000
```

## Storage Optimization

### File System Choice

#### XFS (Recommended)

```bash
# Format with XFS
mkfs.xfs -f -d agcount=32 -l size=512m /dev/nvme0n1

# Mount with optimizations
mount -o noatime,nodiratime,nobarrier,inode64,logbufs=8,logbsize=256k /dev/nvme0n1 /data
```

#### ext4

```bash
# Format with ext4
mkfs.ext4 -E stride=32,stripe-width=256 /dev/nvme0n1

# Mount with optimizations
mount -o noatime,nodiratime,nobarrier,data=writeback /dev/nvme0n1 /data
```


## Troubleshooting Performance Issues

### Common Bottlenecks

#### CPU Bottleneck

Symptoms:
- High CPU usage (> 90%)
- Low disk/network utilization
- Increasing response times

Solutions:
```bash
# Increase worker threads
export WORKERS=16

# Enable CPU performance mode
cpupower frequency-set -g performance

# Scale horizontally
# Deploy multiple IronBucket instances
```

#### Memory Bottleneck

Symptoms:
- High memory usage
- Frequent garbage collection
- OOM kills

Solutions:
```bash
# Reduce cache size
export MAX_MEMORY_CACHE=1073741824  # 1GB

# Optimize buffer pools
export BUFFER_POOL_SIZE=50

# Add swap (emergency only)
fallocate -l 8G /swapfile
chmod 600 /swapfile
mkswap /swapfile
swapon /swapfile
```

#### Disk I/O Bottleneck

Symptoms:
- High disk utilization (iostat shows > 90%)
- High I/O wait
- Slow uploads/downloads

Solutions:
```bash
# Use faster storage
# Move to NVMe or RAID array

# Optimize file system
mount -o remount,noatime,nodiratime /data

# Increase read-ahead
echo 8192 > /sys/block/nvme0n1/queue/read_ahead_kb

# Use separate disks for temp
export TEMP_DIR=/tmpfs/ironbucket
```

#### Network Bottleneck

Symptoms:
- Network interface saturation
- High packet loss
- Connection timeouts

Solutions:
```bash
# Increase network buffers
echo "67108864" > /proc/sys/net/core/rmem_max
echo "67108864" > /proc/sys/net/core/wmem_max

# Enable jumbo frames
ip link set dev eth0 mtu 9000

# Use multiple network interfaces
# Configure bonding or load balancing
```

### Performance Debugging

```bash
# Check system bottlenecks
dstat -cdnmpy 1

# Monitor IronBucket specifically
strace -c -p $(pgrep ironbucket)

# Check file descriptor usage
lsof -p $(pgrep ironbucket) | wc -l

# Analyze network connections
ss -tanp | grep 9000 | wc -l

# Profile memory usage
pmap -x $(pgrep ironbucket)
```

## Best Practices

1. **Start with baseline measurements** before optimization
2. **Optimize one component at a time** to identify improvements
3. **Monitor continuously** in production
4. **Test with realistic workloads** that match your use case
5. **Document configuration changes** for reproducibility
6. **Plan for peak load** with 20-30% headroom
7. **Use dedicated hardware** for production deployments
8. **Implement gradual rollouts** for configuration changes
9. **Keep logs minimal** in production for performance
10. **Regular maintenance** including cache clearing and log rotation

## Optimization Checklist

- [ ] OS kernel parameters tuned
- [ ] File descriptor limits increased
- [ ] CPU governor set to performance
- [ ] Network stack optimized
- [ ] Fast storage (SSD/NVMe) in use
- [ ] File system mounted with optimal flags
- [ ] IronBucket worker threads configured
- [ ] Buffer sizes optimized for workload
- [ ] Caching enabled and sized appropriately
- [ ] Monitoring and alerting configured
- [ ] Load testing completed
- [ ] Bottlenecks identified and addressed
- [ ] Documentation updated with changes

## Next Steps

- [Security Guide](./SECURITY.md) - Secure your optimized deployment
- [Troubleshooting Guide](./TROUBLESHOOTING.md) - Resolve performance issues
- [Configuration Guide](./CONFIGURATION.md) - Fine-tune settings