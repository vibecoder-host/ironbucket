# IronBucket Cluster Mode Documentation

## Overview

IronBucket supports a distributed cluster mode with Write-Ahead Log (WAL) based replication. This mode enables high availability, data redundancy, and load distribution across multiple nodes while maintaining S3 compatibility.

## Architecture

The cluster consists of the following components:

1. **IronBucket Nodes**: Multiple instances of the IronBucket storage server
2. **WAL (Write-Ahead Log)**: Transaction log for each node recording all operations
3. **Replicator Service**: Daemon that reads WAL entries and replicates data between nodes
4. **Nginx Load Balancer**: Distributes requests across nodes with session affinity
5. **Shared Storage**: Each node maintains its own storage directory

### Design Principles

- **Shared-Nothing Architecture**: Nodes operate independently with their own storage
- **WAL-Based Replication**: All operations are logged before execution and replicated asynchronously
- **Direct Disk Writes**: Replication bypasses the API layer to prevent loops
- **Session Affinity**: Clients consistently connect to the same node for better cache utilization

## Deployment Options

### 1. Single Node with WAL (docker-compose.wal.yml)

Enables WAL logging on a single node for audit trails and future cluster expansion.

```bash
docker compose -f docker-compose.wal.yml up -d --build
```

**Use Case**: Development, testing WAL functionality, preparing for cluster deployment

### 2. Two-Node WAL Cluster (docker-compose.cluster-wal.yml)

Full cluster setup with two nodes, replication, and load balancing.

```bash
docker compose -f docker-compose.cluster-wal.yml up -d --build
```

**Use Case**: Production deployment, high availability, data redundancy

### 3. Multi-Node WAL Cluster (Extended Configuration)

For clusters with 3 or more nodes, extend the docker-compose.cluster-wal.yml:

```yaml
# Example for node3 in a 3-node cluster
ironbucket-node3:
  environment:
    - NODE_ID=node-3
    - CLUSTER_NODES=ironbucket-node1:9000,ironbucket-node2:9000

replicator-node3:
  environment:
    - NODE_ID=node-3
    - CLUSTER_NODES=ironbucket-node1:9000,ironbucket-node2:9000
```

Each node should list all OTHER nodes in its CLUSTER_NODES (excluding itself).


## Configuration

### Environment Variables

Each node requires the following environment variables:

```yaml
environment:
  - ACCESS_KEY=root
  - SECRET_KEY=xxxxxxxxxxxxxxx
  - STORAGE_PATH=/s3
  - WAL_ENABLED=true           # Enable WAL logging
  - WAL_PATH=/wal/wal.log      # WAL file location
  - NODE_ID=node-1             # Unique node identifier
```

### Replicator Configuration

The replicator service requires:

```yaml
environment:
  - NODE_ID=node-1
  - WAL_PATH=/wal/wal.log
  - STATE_PATH=/state/replicator.state
  - STORAGE_PATH=/s3
  - CLUSTER_NODES=ironbucket-node2:9000  # Single node
  # OR for multiple nodes:
  - CLUSTER_NODES=ironbucket-node2:9000,ironbucket-node3:9000,ironbucket-node4:9000
  - BATCH_INTERVAL=5000                  # Milliseconds between batches
  - MAX_BATCH_SIZE=100                   # Maximum entries per batch
```

**Note**: CLUSTER_NODES supports multiple comma-separated nodes. Each replicator will send its WAL entries to all specified nodes.

### Volume Mounts

Each node requires persistent volumes:

```yaml
volumes:
  - ./cluster-wal/node1/s3:/s3           # Data storage
  - ./cluster-wal/node1/wal:/wal         # WAL logs
  - ./cluster-wal/node1/state:/state     # Replicator state
  - ./cluster-wal:/cluster-wal           # Shared for direct replication
```

### Load Balancer Configuration

The nginx load balancer uses IP hash for session affinity:

```nginx
upstream ironbucket {
    ip_hash;  # Session affinity
    server ironbucket-node1:9000;
    server ironbucket-node2:9000;
}
```

## Operations

### Starting the Cluster

```bash
# Start the WAL cluster
docker compose -f docker-compose.cluster-wal.yml up -d --build

# Verify all containers are running
docker ps | grep ironbucket

# Check logs
docker logs ironbucket-ironbucket-node1-1
docker logs ironbucket-replicator-node1-1
```

### Stopping the Cluster

```bash
# Graceful shutdown
docker compose -f docker-compose.cluster-wal.yml down

# Force stop (not recommended)
docker compose -f docker-compose.cluster-wal.yml kill
```

### Monitoring Replication

Check replication status:

```bash
# View replicator logs
docker logs ironbucket-replicator-node1-1 -f

# Check replicator state
cat cluster-wal/node1/state/replicator.state | python3 -m json.tool

# Monitor WAL size
ls -lh cluster-wal/node1/wal/wal.log
```

### Accessing the Cluster

The cluster is accessible through the nginx load balancer:

```bash
# Using AWS CLI
export AWS_ACCESS_KEY_ID=root
export AWS_SECRET_ACCESS_KEY=xxxxxxxxxxxxxxx
aws --endpoint-url=http://localhost:20000 s3 ls

# Using curl (requires authentication)
curl -X GET http://localhost:20000/
```

## WAL Management

### WAL Entry Format

Each WAL entry contains:
- Operation type (PUT, DELETE, CREATE_BUCKET, DELETE_BUCKET)
- Node ID
- Sequence number
- Timestamp
- Operation-specific data

Example:
```
PUT	node-1	123	1759184567899	mybucket	myfile.jpg	123851	733085887746fe7a20e04fb0bd75b498
```

### Sequence Management

The WAL system maintains sequence numbers for ordering operations:
- Sequences are persistent across restarts
- Stored in `.sequence` file for fast recovery
- Monotonically increasing per node

### Performance Optimization

The WAL implementation includes several optimizations:

1. **Fast Startup**: Reads sequence from state file instead of scanning entire WAL
2. **Batched Writes**: Groups operations for efficient disk I/O
3. **Delayed Flushing**: Reduces synchronous disk operations
4. **Partial WAL Reading**: Only reads last 10KB on startup

## Troubleshooting

### Common Issues

#### 1. Replication Not Working

Check replicator logs:
```bash
docker logs ironbucket-replicator-node1-1 | grep ERROR
```

Verify WAL entries are being created:
```bash
tail -f cluster-wal/node1/wal/wal.log
```

#### 2. Duplicate Sequence Numbers

Clear replicator state and restart:
```bash
rm cluster-wal/node1/state/replicator.state
docker restart ironbucket-replicator-node1-1
```

#### 3. High Memory Usage

WAL files grow continuously. Consider implementing rotation:
```bash
# Check WAL size
du -h cluster-wal/*/wal/wal.log

# Rotate manually (ensure replicator is stopped)
mv cluster-wal/node1/wal/wal.log cluster-wal/node1/wal/wal.log.old
touch cluster-wal/node1/wal/wal.log
```

#### 4. Node Out of Sync

Force full resync:
```bash
# Stop replicator
docker stop ironbucket-replicator-node1-1

# Clear state
rm cluster-wal/node1/state/replicator.state

# Restart replicator
docker start ironbucket-replicator-node1-1
```

### Debug Mode

Enable debug logging:
```bash
# Set RUST_LOG environment variable
docker run -e RUST_LOG=debug ...
```

## Performance Tuning

### WAL Flush Interval

Adjust the flush frequency in `src/wal.rs`:
- Default: 5 seconds
- High throughput: 10-30 seconds
- High durability: 1-2 seconds

### Replicator Batch Size

Configure in docker-compose:
```yaml
environment:
  - MAX_BATCH_SIZE=100    # Default
  - MAX_BATCH_SIZE=1000   # High throughput
  - MAX_BATCH_SIZE=10     # Low latency
```

### Network Optimization

For better performance across nodes:
1. Use dedicated network interfaces
2. Enable jumbo frames if supported
3. Consider using SSD for WAL storage

## Backup and Recovery

### Backing Up

1. **WAL Backup**: Essential for point-in-time recovery
```bash
tar czf wal-backup-$(date +%Y%m%d).tar.gz cluster-wal/*/wal/
```

2. **Data Backup**: Current state of all objects
```bash
tar czf data-backup-$(date +%Y%m%d).tar.gz cluster-wal/*/s3/
```

### Recovery

1. **From WAL**: Replay WAL entries to rebuild state
2. **From Snapshot**: Restore data directories and restart

## Security Considerations

1. **Authentication**: Always use strong SECRET_KEY
2. **Network**: Isolate cluster network from public access
3. **Encryption**: Consider TLS for inter-node communication
4. **Access Control**: Limit replicator permissions to required operations

## Limitations

- No automatic failover (requires external orchestration)
- No built-in WAL rotation (manual intervention required)
- Session affinity may cause uneven load distribution
- Replication is eventually consistent, not strongly consistent

## Future Enhancements

- [ ] Automatic WAL rotation and archival
- [ ] Multi-node consensus for strong consistency
- [ ] Automatic failover and recovery
- [ ] Read replicas for improved read performance
- [ ] Compression for WAL entries
- [ ] Metrics and monitoring dashboard