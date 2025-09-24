# Erasure Coding Implementation Plan for IronBucket Cluster

## Overview
Implement Reed-Solomon erasure coding to provide efficient data redundancy with lower storage overhead compared to full replication. This will allow IronBucket to store data across multiple nodes with configurable data/parity shards, enabling recovery from node failures while using less storage than traditional replication.

## Phase 1: Core Erasure Coding Library (Week 1)

### 1.1 Add Reed-Solomon Dependencies
- Add `reed-solomon-erasure` crate to Cargo.toml
- Add `blake3` for fast hashing/checksums
- Add supporting crates for matrix operations

### 1.2 Create Erasure Coding Module
- Create `src/erasure/mod.rs` with core erasure coding logic
- Implement `ErasureCoder` trait with encode/decode methods
- Support configurable data shards (k) and parity shards (m)
- Implement chunk size calculation based on object size

### 1.3 Implement Reed-Solomon Engine
- Create `src/erasure/reed_solomon.rs`
- Wrapper around reed-solomon-erasure library
- Support for systematic encoding (data shards remain unmodified)
- Implement Galois Field operations for efficient encoding

## Phase 2: Storage Integration (Week 2)

### 2.1 Modify Storage Backend
- Update `StorageBackend` trait to support shard operations
- Add methods: `put_shard()`, `get_shard()`, `delete_shard()`
- Implement shard metadata tracking

### 2.2 Create Shard Manager
- Create `src/erasure/shard_manager.rs`
- Track shard placement across nodes
- Implement shard naming convention: `{object_id}.{shard_index}`
- Manage shard metadata and checksums

### 2.3 Implement Erasure Storage Strategy
- Create `src/storage/erasure_storage.rs`
- Implement object chunking for large objects
- Support both full-object and chunked erasure coding
- Add configurable threshold for erasure coding activation

## Phase 3: Cluster Distribution (Week 3)

### 3.1 Enhance Cluster Manager
- Update `src/cluster.rs` to support erasure coding operations
- Implement shard distribution algorithm
- Add rack/zone awareness for shard placement
- Ensure no two shards of same object on same node

### 3.2 Create Shard Placement Strategy
- Create `src/erasure/placement.rs`
- Implement consistent hashing for shard distribution
- Support configurable placement policies:
  - Random placement
  - Round-robin placement
  - Locality-aware placement
  - Anti-affinity rules

### 3.3 Implement Shard Replication
- Support hybrid mode: erasure coding + replication
- Allow critical shards to have replicas
- Implement priority-based shard recovery

## Phase 4: Read/Write Operations (Week 4)

### 4.1 Implement Erasure Write Path
```rust
// src/erasure/operations.rs
async fn write_with_erasure(
    object: Bytes,
    bucket: &str,
    key: &str,
    config: ErasureConfig
) -> Result<()> {
    // 1. Split object into k data shards
    // 2. Generate m parity shards
    // 3. Distribute k+m shards across nodes
    // 4. Store metadata mapping
}
```

### 4.2 Implement Erasure Read Path
```rust
async fn read_with_erasure(
    bucket: &str,
    key: &str
) -> Result<Bytes> {
    // 1. Read metadata to get shard locations
    // 2. Fetch minimum k shards (parallel)
    // 3. Reconstruct object if needed
    // 4. Return complete object
}
```

### 4.3 Implement Degraded Read
- Support reading with missing shards
- Implement parallel shard fetching
- Add timeout and retry logic
- Cache reconstructed objects

## Phase 5: Recovery and Repair (Week 5)

### 5.1 Implement Shard Recovery
- Create `src/erasure/recovery.rs`
- Detect missing/corrupted shards
- Reconstruct missing shards from available ones
- Re-distribute recovered shards

### 5.2 Background Repair Process
- Implement periodic scrubbing
- Verify shard checksums
- Detect and repair bit rot
- Track repair statistics

### 5.3 Node Failure Handling
- Detect node failures via gossip protocol
- Trigger shard reconstruction for failed node
- Implement priority queue for recovery
- Support configurable recovery bandwidth

## Phase 6: Configuration and Tuning (Week 6)

### 6.1 Add Configuration Options
```rust
// Update src/config.rs
pub struct ErasureConfig {
    pub enabled: bool,
    pub data_shards: usize,      // k value
    pub parity_shards: usize,    // m value
    pub min_object_size: u64,    // Minimum size for erasure
    pub shard_size: u64,         // Fixed shard size
    pub recovery_threads: usize, // Parallel recovery
    pub placement_policy: String,// Placement strategy
}
```

### 6.2 Dynamic Configuration
- Support per-bucket erasure settings
- Allow storage class selection (STANDARD, REDUCED_REDUNDANCY)
- Implement automatic mode selection based on object size

### 6.3 Performance Tuning
- Add configurable encoding/decoding thread pool
- Implement shard caching layer
- Support vectorized operations for encoding
- Add metrics for encoding/decoding performance

## Phase 7: Testing and Validation (Week 7)

### 7.1 Unit Tests
- Test erasure encoding/decoding
- Test shard distribution algorithms
- Test recovery mechanisms
- Test edge cases (single shard, large objects)

### 7.2 Integration Tests
- Multi-node erasure coding tests
- Node failure simulation
- Network partition tests
- Performance benchmarks

### 7.3 Chaos Testing
- Random shard corruption
- Multiple node failures
- Network delays during reconstruction
- Storage exhaustion scenarios

## Technical Details

### Erasure Coding Parameters
- **Default Configuration**: 6 data shards + 3 parity shards (6+3)
- **Storage Efficiency**: 150% overhead vs 200% for 2x replication
- **Fault Tolerance**: Can survive 3 node failures
- **Minimum Nodes**: 9 nodes for optimal distribution

### Performance Targets
- **Encoding Speed**: >500 MB/s for large objects
- **Decoding Speed**: >1 GB/s for normal reads
- **Recovery Speed**: >100 MB/s per node
- **CPU Overhead**: <10% for encoding operations

### Storage Classes
1. **STANDARD**: Full replication (backward compatible)
2. **ERASURE_6_3**: 6+3 erasure coding (default)
3. **ERASURE_10_4**: 10+4 for higher durability
4. **ERASURE_4_2**: 4+2 for smaller clusters

## Dependencies to Add

```toml
# Cargo.toml additions
reed-solomon-erasure = "6.0"
blake3 = "1.5"
rayon = "1.7"              # Parallel processing
crossbeam-channel = "0.5"  # Shard coordination
bitvec = "1.0"            # Bit manipulation
prometheus = "0.13"        # Metrics
```

## File Structure

```
src/erasure/
├── mod.rs                 # Main erasure module
├── reed_solomon.rs        # Reed-Solomon implementation
├── shard_manager.rs       # Shard lifecycle management
├── placement.rs           # Shard placement strategies
├── operations.rs          # Read/write operations
├── recovery.rs            # Recovery and repair
└── metrics.rs            # Performance metrics
```

## Monitoring Metrics

- `erasure_encode_duration_seconds` - Encoding time
- `erasure_decode_duration_seconds` - Decoding time
- `erasure_shards_total` - Total shards in cluster
- `erasure_shards_missing` - Missing shards count
- `erasure_recovery_operations_total` - Recovery operations
- `erasure_storage_efficiency` - Storage efficiency ratio

## Success Criteria

1. Successfully encode/decode objects with configurable parameters
2. Survive m node failures without data loss
3. Achieve 50% storage savings vs 3x replication
4. Maintain <10ms additional latency for reads
5. Support smooth migration from replication to erasure coding
6. Pass all integration and chaos tests

## Implementation Example

### Basic Erasure Coding Flow

```rust
use reed_solomon_erasure::galois_8::ReedSolomon;
use bytes::Bytes;

pub struct ErasureCoder {
    encoder: ReedSolomon,
    data_shards: usize,
    parity_shards: usize,
}

impl ErasureCoder {
    pub fn new(data_shards: usize, parity_shards: usize) -> Result<Self> {
        let encoder = ReedSolomon::new(data_shards, parity_shards)?;
        Ok(Self {
            encoder,
            data_shards,
            parity_shards,
        })
    }

    pub fn encode(&self, data: Bytes) -> Result<Vec<Vec<u8>>> {
        let shard_size = (data.len() + self.data_shards - 1) / self.data_shards;
        let mut shards = vec![vec![0u8; shard_size]; self.data_shards + self.parity_shards];

        // Split data into shards
        for (i, chunk) in data.chunks(shard_size).enumerate() {
            shards[i][..chunk.len()].copy_from_slice(chunk);
        }

        // Generate parity shards
        self.encoder.encode(&mut shards)?;
        Ok(shards)
    }

    pub fn decode(&self, shards: Vec<Option<Vec<u8>>>) -> Result<Bytes> {
        let mut shards = shards;
        self.encoder.reconstruct(&mut shards)?;

        // Concatenate data shards
        let mut result = Vec::new();
        for i in 0..self.data_shards {
            if let Some(shard) = &shards[i] {
                result.extend_from_slice(shard);
            }
        }

        Ok(Bytes::from(result))
    }
}
```

### Cluster Distribution Example

```rust
use std::collections::HashMap;

pub struct ShardDistributor {
    nodes: Vec<String>,
    placement_strategy: PlacementStrategy,
}

impl ShardDistributor {
    pub async fn distribute_shards(
        &self,
        object_id: &str,
        shards: Vec<Vec<u8>>,
    ) -> Result<HashMap<usize, String>> {
        let mut placement = HashMap::new();
        let selected_nodes = self.select_nodes(shards.len());

        for (shard_idx, (shard_data, node)) in shards.iter().zip(selected_nodes.iter()).enumerate() {
            // Send shard to node
            self.send_shard_to_node(node, object_id, shard_idx, shard_data).await?;
            placement.insert(shard_idx, node.clone());
        }

        Ok(placement)
    }

    fn select_nodes(&self, count: usize) -> Vec<String> {
        match self.placement_strategy {
            PlacementStrategy::Random => self.random_selection(count),
            PlacementStrategy::RoundRobin => self.round_robin_selection(count),
            PlacementStrategy::LocalityAware => self.locality_aware_selection(count),
        }
    }
}
```

## Migration Path

### Phase 1: Preparation
- Deploy erasure coding module alongside existing replication
- Test with non-critical data
- Gather performance metrics

### Phase 2: Gradual Migration
- Enable erasure coding for new objects above threshold size
- Keep existing replicated objects unchanged
- Monitor storage efficiency improvements

### Phase 3: Full Migration
- Background job to convert replicated objects to erasure coded
- Maintain fallback to replication if needed
- Complete migration over several weeks

### Phase 4: Optimization
- Tune parameters based on workload
- Optimize shard placement
- Implement advanced features (locality groups, tiering)

## Conclusion

This erasure coding implementation will significantly improve IronBucket's storage efficiency while maintaining high availability and durability. The phased approach ensures a smooth transition from replication to erasure coding, with comprehensive testing at each stage.