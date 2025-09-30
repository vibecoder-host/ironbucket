# IronBucket Cluster Implementation - New Strategy (Shared Nothing)

## Overview
Complete redesign of cluster implementation using a shared-nothing architecture. The main IronBucket process remains untouched, with a separate replication daemon handling all cluster operations through WAL (Write-Ahead Log) processing.

## Architecture Principles
1. **Zero impact on single-node performance** - Main process runs exactly as single-node
2. **Separate replication daemon** - Independent container/process for cluster operations
3. **WAL-based replication** - Read operations from write-ahead log
4. **Batch processing** - Process events in batches every 5 seconds max
5. **Smart replication** - Skip downloading files that are created and deleted in same batch
6. **No replication loops** - Replicated operations bypass WAL to prevent infinite loops

## Data Flow (Preventing Loops)

```
Node 1 (Source):
┌─────────────────────────────────────────┐
│  Client PUT /bucket/key              │
│     ↓                                │
│  IronBucket Process                  │
│     ├→ Write to disk                 │
│     └→ Write to WAL (LOCAL OPS ONLY) │
│           ↓                          │
│  Replicator Daemon                   │
│     ├→ Read WAL                      │
│     └→ Send to other Nodes           │
└─────────────────────────────────────────┘

Node 2 (Target):
┌──────────────────────────────────────────┐
│  Replicator Daemon                   │
│     ├→ Receive event from Node 1     │
│     ├→ fetch file from Node 1        │
│     ├→ Write DIRECTLY to disk        │
│     └→ NO WAL ENTRY (prevents loop)  │
│                                      │
│  IronBucket Process                  │
│     └→ Serves the file normally      │
└─────────────────────────────────────────┘

CRITICAL: Only operations from clients create WAL entries.
         Replicated operations go directly to disk.
```

## Components

### 1. Main IronBucket Process (No Changes)
- Runs exactly as single-node version
- No cluster code, no performance impact
- Writes operations to WAL file
- Serves S3 API requests

### 2. WAL (Write-Ahead Log) System
```rust
// WAL entry format
struct WALEntry {
    node_id: String,        // CRITICAL: Node that created this event (e.g., "node-1")
    sequence_id: u64,       // Unique sequence number for this node
    timestamp: u64,
    operation: Operation,
    bucket: String,
    key: String,
    size: Option<u64>,
    etag: Option<String>,
    metadata: Option<HashMap<String, String>>,
}

enum Operation {
    PutObject,
    DeleteObject,
    CreateBucket,
    DeleteBucket,
    PutObjectMetadata,
}
```

### 3. Replication Daemon (New Container)
Independent process that:
- Monitors WAL file for new entries
- Tracks last transmitted sequence ID
- Batches events for transmission
- Receives events from other nodes
- Downloads missing objects

## Implementation Plan

### Phase 1: WAL Implementation in Main Process
- [ ] Add WAL writer to IronBucket (append-only log)
- [ ] Log all write operations (PUT, DELETE, etc.)
- [ ] Implement WAL rotation (e.g., daily files)
- [ ] Add WAL configuration (path, max size, rotation)

### Phase 2: Replication Daemon Base
- [ ] Create new Rust binary: `ironbucket-replicator`
- [ ] Implement WAL reader with tail -f like behavior
- [ ] Track last processed sequence ID in state file
- [ ] Create event buffer for batching

### Phase 3: Cluster Communication
- [ ] Implement cluster membership discovery (via environment variables)
- [ ] Create gRPC/HTTP API for inter-node communication
- [ ] Implement event broadcasting to other nodes
- [ ] Add event deduplication (by sequence ID + node ID)
- [ ] **CRITICAL**: Mark replicated operations to prevent replication loops

### Phase 4: Smart Replication Logic
- [ ] Batch events every 5 seconds maximum
- [ ] Analyze batch for create/delete pairs - skip downloading
- [ ] Implement parallel object downloads
- [ ] Add retry logic with exponential backoff
- [ ] Handle network partitions gracefully

### Phase 5: Docker Compose Setup
```yaml
version: '3.8'
services:
  ironbucket:
    build: .
    image: ironbucket:latest
    volumes:
      - ./data:/s3
      - ./wal:/wal  # WAL directory shared with replicator
    environment:
      - ENABLE_WAL=true
      - WAL_PATH=/wal
    ports:
      - "9000:9000"

  replicator:
    build:
      context: .
      dockerfile: Dockerfile.replicator
    image: ironbucket-replicator:latest
    volumes:
      - ./data:/s3:ro  # Read-only access to data
      - ./wal:/wal:ro  # Read-only access to WAL
      - ./replicator-state:/state
    environment:
      - NODE_ID=node-1
      - CLUSTER_NODES=node-2:7000,node-3:7000
      - WAL_PATH=/wal
      - STATE_PATH=/state
      - BATCH_INTERVAL_MS=5000
    depends_on:
      - ironbucket
```

## Preventing Replication Loops

### Critical Design Principle
**Only local operations should be logged to WAL and replicated. Operations that come from the replication system must NOT generate new WAL entries.**

### Implementation Approaches

#### Option 1: Separate Write Path for Replication (Recommended)
The replicator daemon writes directly to disk, bypassing the main IronBucket API:
```rust
// In replicator daemon - direct disk writes, no WAL
async fn apply_replicated_object(bucket: &str, key: &str, data: &[u8]) {
    // Direct filesystem write - bypasses IronBucket completely
    let path = format!("/s3/{}/{}", bucket, key);
    fs::create_dir_all(Path::new(&path).parent().unwrap()).await?;
    fs::write(&path, data).await?;
    // NO WAL entry created - this is a replicated operation
}
```

#### Option 2: Replication Flag in Request Headers
Mark requests coming from replicator with special header:
```rust
// In IronBucket main process
pub async fn handle_put_object(headers: HeaderMap, ...) {
    // Check if this is a replicated operation
    let is_replicated = headers.get("X-Ironbucket-Replicated").is_some();

    // Store the object
    fs::write(&object_path, &data).await?;

    // Only log to WAL if it's a local operation
    if !is_replicated {
        wal_writer.log_put(&bucket, &key, data.len() as u64);
    }
}
```

#### Option 3: Separate Port for Replication (Most Robust)
- Port 9000: Client API (logs to WAL)
- Port 9001: Replication API (no WAL logging)

```rust
// Two different routers in main process
let client_router = Router::new()
    .route("/:bucket/*key", put(handle_client_put)); // Logs to WAL

let replication_router = Router::new()
    .route("/:bucket/*key", put(handle_replication_put)); // NO WAL

// Start both servers
tokio::join!(
    axum::Server::bind(&"0.0.0.0:9000".parse()?).serve(client_router),
    axum::Server::bind(&"0.0.0.0:9001".parse()?).serve(replication_router)
);
```

### Recommended Solution: Direct Disk Writes
The cleanest approach is **Option 1** - the replicator writes directly to disk:

**Advantages:**
- Zero performance impact on main process
- No code changes to IronBucket at all
- Clear separation of concerns
- No possibility of loops

**Implementation:**
```rust
// Replicator daemon receives events from other nodes
async fn handle_replication_batch(batch: ReplicationBatch) {
    for event in batch.entries {
        match event.operation {
            Operation::PutObject => {
                // Download object from source node
                let data = download_object(&event.node_id, &event.bucket, &event.key).await?;

                // Write directly to filesystem - NO WAL, NO API call
                let path = format!("{}/{}/{}", storage_path, event.bucket, event.key);
                fs::create_dir_all(Path::new(&path).parent().unwrap()).await?;
                fs::write(&path, data).await?;

                info!("Replicated {}/{} from node {}", event.bucket, event.key, event.node_id);
            }
            Operation::DeleteObject => {
                // Delete directly from filesystem
                let path = format!("{}/{}/{}", storage_path, event.bucket, event.key);
                fs::remove_file(&path).await?;
            }
        }
    }
}
```

## Detailed Implementation

### WAL Writer (in main process)
```rust
// ULTRA-MINIMAL impact implementation
// Uses lock-free ring buffer, NO serialization in hot path, batch flush every 1 second

use crossbeam::channel::{bounded, Sender};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct WALWriter {
    // Lock-free channel to background writer thread
    sender: Sender<WALOp>,
    sequence: AtomicU64,
}

// Minimal data copied in hot path - just references
enum WALOp {
    Put { bucket: String, key: String, size: u64 },
    Delete { bucket: String, key: String },
}

impl WALWriter {
    pub fn new(path: PathBuf) -> Self {
        // Bounded channel to prevent memory bloat
        let (sender, receiver) = bounded(10000);

        // Dedicated thread for WAL writing - completely separate from tokio runtime
        std::thread::spawn(move || {
            let mut file = BufWriter::with_capacity(
                1024 * 1024, // 1MB buffer
                File::create(path).unwrap()
            );
            let mut batch = Vec::with_capacity(1000);
            let mut last_flush = Instant::now();

            loop {
                // Collect operations for up to 1 second
                let timeout = Duration::from_millis(100);
                while let Ok(op) = receiver.recv_timeout(timeout) {
                    batch.push(op);

                    // Flush every second OR if batch is large
                    if last_flush.elapsed() >= Duration::from_secs(1) || batch.len() >= 1000 {
                        // Write batch to disk
                        for op in batch.drain(..) {
                            // Simple binary format, no JSON overhead
                            match op {
                                WALOp::Put { bucket, key, size } => {
                                    writeln!(file, "P\t{}\t{}\t{}", bucket, key, size).unwrap();
                                }
                                WALOp::Delete { bucket, key } => {
                                    writeln!(file, "D\t{}\t{}", bucket, key).unwrap();
                                }
                            }
                        }
                        file.flush().unwrap();
                        last_flush = Instant::now();
                    }
                }
            }
        });

        WALWriter {
            sender,
            sequence: AtomicU64::new(0),
        }
    }

    // CRITICAL: This must be FAST - just copy strings and send
    #[inline(always)]
    pub fn log_put(&self, bucket: &str, key: &str, size: u64) {
        // Fire and forget - never blocks
        let _ = self.sender.try_send(WALOp::Put {
            bucket: bucket.to_string(),
            key: key.to_string(),
            size,
        });
        // If channel is full, we drop the event (acceptable for eventual consistency)
    }

    #[inline(always)]
    pub fn log_delete(&self, bucket: &str, key: &str) {
        let _ = self.sender.try_send(WALOp::Delete {
            bucket: bucket.to_string(),
            key: key.to_string(),
        });
    }
}
```

### Replicator Event Processor
```rust
pub struct EventProcessor {
    buffer: Vec<WALEntry>,
    last_flush: Instant,
    max_batch_interval: Duration,
}

impl EventProcessor {
    pub async fn process_batch(&mut self, events: Vec<WALEntry>) {
        // Smart batch analysis
        let mut operations: HashMap<(String, String), Vec<WALEntry>> = HashMap::new();

        for event in events {
            let key = (event.bucket.clone(), event.key.clone());
            operations.entry(key).or_default().push(event);
        }

        // Filter out create/delete pairs
        let mut to_replicate = Vec::new();
        for ((bucket, key), ops) in operations {
            let has_create = ops.iter().any(|e| matches!(e.operation, Operation::PutObject));
            let has_delete = ops.iter().any(|e| matches!(e.operation, Operation::DeleteObject));

            if has_create && has_delete {
                // Skip - object was created and deleted in same batch
                info!("Skipping replication for {}/{} - created and deleted in same batch", bucket, key);
                continue;
            }

            // Take only the last operation for this key
            if let Some(last_op) = ops.into_iter().last() {
                to_replicate.push(last_op);
            }
        }

        // Process remaining operations
        self.replicate_operations(to_replicate).await;
    }
}
```

### Network Protocol
```protobuf
// replication.proto
syntax = "proto3";

message ReplicationBatch {
    string source_node_id = 1;  // CRITICAL: Identifies where events originated
    uint64 batch_id = 2;
    repeated WALEntry entries = 3;
    uint64 timestamp = 4;
}

message WALEntry {
    string source_node_id = 1;  // Original node that created this event
    uint64 sequence_id = 2;
    uint64 timestamp = 3;
    string operation = 4;
    string bucket = 5;
    string key = 6;
    optional uint64 size = 7;
    optional string etag = 8;
}

service Replication {
    rpc SendBatch(ReplicationBatch) returns (Acknowledgment);
    rpc GetObject(ObjectRequest) returns (stream ObjectData);
}

// IMPORTANT: Node receiving ReplicationBatch MUST:
// 1. Apply changes directly to disk (no WAL)
// 2. NOT forward these events to other nodes
// 3. Track source_node_id + sequence_id to prevent duplicates
```

## Performance Considerations

1. **WAL Write Performance**
   - **Near-zero impact**: Lock-free channel with try_send (never blocks)
   - **No serialization in hot path**: Just string copies to channel
   - **Separate thread**: WAL writer runs on dedicated OS thread, not tokio runtime
   - **Batch flush**: Disk writes happen every 1 second in background
   - **Expected overhead**: <0.1% performance impact (just memory allocation for strings)

2. **Replicator Efficiency**
   - Batch processing reduces network overhead
   - Parallel downloads for multiple objects
   - Connection pooling for node communication
   - Skip unnecessary operations (create/delete pairs)

3. **Resource Usage**
   - Replicator uses minimal CPU when idle
   - Memory usage proportional to batch size
   - Network bandwidth used efficiently with batching

## Monitoring & Observability

### Metrics to Track
- WAL write latency (should be <1ms)
- Replication lag (sequence ID difference)
- Batch size and processing time
- Objects replicated per second
- Network bandwidth usage
- Failed replication attempts

### Health Checks
```rust
// Health endpoint for replicator
GET /health
{
    "status": "healthy",
    "last_wal_sequence": 12345,
    "last_replicated_sequence": 12340,
    "lag": 5,
    "connected_nodes": 2,
    "last_batch_time": "2024-03-20T10:00:00Z"
}
```

## Testing Strategy

1. **Performance Testing**
   - Verify zero impact on single-node performance
   - Measure replication latency under load
   - Test batch optimization effectiveness

2. **Failure Scenarios**
   - Network partition handling
   - Node failure and recovery
   - WAL corruption recovery
   - Replicator crash and restart

3. **Correctness Testing**
   - Verify eventual consistency
   - Test create/delete optimization
   - Validate deduplication logic

## Migration Path

1. **Stage 1**: Deploy WAL writer (no replicator yet)
   - Monitor WAL performance impact (should be negligible)
   - Validate WAL format and rotation

2. **Stage 2**: Deploy replicator in monitoring mode
   - Read WAL but don't replicate
   - Measure resource usage
   - Validate batch processing logic

3. **Stage 3**: Enable replication between two nodes
   - Start with low traffic
   - Monitor replication lag
   - Validate data consistency

4. **Stage 4**: Full cluster deployment
   - Scale to multiple nodes
   - Enable production traffic
   - Monitor all metrics

## Expected Performance

- **Single-node performance**: 100% of baseline (no impact)
- **Replication overhead**: <5% CPU, <100MB memory for replicator
- **Replication lag**: <10 seconds under normal load
- **Network efficiency**: 10-100x better than synchronous replication
- **Storage efficiency**: Skip redundant operations automatically

## Configuration

```yaml
# ironbucket.yaml
wal:
  enabled: true
  path: /wal
  max_file_size: 1GB
  rotation_interval: 24h
  compression: none  # Can add compression later

# replicator.yaml
replicator:
  node_id: node-1
  cluster_nodes:
    - node-2:7000
    - node-3:7000
  wal_path: /wal
  state_path: /state
  batch:
    max_interval_ms: 5000
    max_size: 1000
  network:
    connection_timeout_ms: 5000
    request_timeout_ms: 30000
    max_concurrent_downloads: 10
  retry:
    max_attempts: 3
    backoff_ms: 1000
```

## Success Criteria

1. **Zero performance impact** on single-node operations
2. **Eventual consistency** achieved within 30 seconds
3. **Network bandwidth** reduced by 10x vs synchronous replication
4. **Automatic optimization** of create/delete pairs
5. **Robust failure handling** with automatic recovery

## Next Steps

1. Implement minimal WAL writer in IronBucket
2. Create replicator daemon skeleton
3. Test WAL performance impact
4. Implement batch processing logic
5. Add network communication
6. Deploy and test two-node cluster
7. Scale to multi-node setup