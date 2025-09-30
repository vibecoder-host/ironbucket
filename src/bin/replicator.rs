use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use reqwest::Client;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WALEntry {
    node_id: String,
    sequence_id: u64,
    timestamp: u64,
    operation: String,
    bucket: String,
    key: String,
    size: Option<u64>,
    etag: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReplicatorState {
    last_processed_position: u64,
    last_processed_sequence: HashMap<String, u64>, // node_id -> sequence
    last_flush: u64,
}

#[derive(Debug, Clone)]
struct ReplicatorConfig {
    node_id: String,
    cluster_nodes: Vec<String>,
    wal_path: PathBuf,
    state_path: PathBuf,
    storage_path: PathBuf,
    batch_interval_ms: u64,
    max_batch_size: usize,
}

impl ReplicatorConfig {
    fn from_env() -> Self {
        let node_id = std::env::var("NODE_ID")
            .unwrap_or_else(|_| "node-1".to_string());

        let cluster_nodes = std::env::var("CLUSTER_NODES")
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        let wal_path = PathBuf::from(
            std::env::var("WAL_PATH").unwrap_or_else(|_| "/wal".to_string())
        ).join("wal.log");

        let state_path = PathBuf::from(
            std::env::var("STATE_PATH").unwrap_or_else(|_| "/state".to_string())
        ).join("replicator.state");

        let storage_path = PathBuf::from(
            std::env::var("STORAGE_PATH").unwrap_or_else(|_| "/s3".to_string())
        );

        let batch_interval_ms = std::env::var("BATCH_INTERVAL_MS")
            .unwrap_or_else(|_| "5000".to_string())
            .parse()
            .unwrap_or(5000);

        let max_batch_size = std::env::var("MAX_BATCH_SIZE")
            .unwrap_or_else(|_| "1000".to_string())
            .parse()
            .unwrap_or(1000);

        ReplicatorConfig {
            node_id,
            cluster_nodes,
            wal_path,
            state_path,
            storage_path,
            batch_interval_ms,
            max_batch_size,
        }
    }
}

struct Replicator {
    config: ReplicatorConfig,
    state: ReplicatorState,
    http_client: Client,
    event_buffer: Vec<WALEntry>,
    seen_events: HashSet<(String, u64)>, // (node_id, sequence_id)
}

impl Replicator {
    fn new(config: ReplicatorConfig) -> Self {
        let state = Self::load_state(&config.state_path);
        let http_client = Client::builder()
            .timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(10)
            .build()
            .expect("Failed to create HTTP client");

        Replicator {
            config,
            state,
            http_client,
            event_buffer: Vec::new(),
            seen_events: HashSet::new(),
        }
    }

    fn load_state(path: &Path) -> ReplicatorState {
        if path.exists() {
            match fs::read_to_string(path) {
                Ok(content) => {
                    match serde_json::from_str(&content) {
                        Ok(state) => {
                            info!("Loaded replicator state from {:?}", path);
                            return state;
                        }
                        Err(e) => warn!("Failed to parse state file: {}", e),
                    }
                }
                Err(e) => warn!("Failed to read state file: {}", e),
            }
        }

        info!("Starting with fresh replicator state");
        ReplicatorState {
            last_processed_position: 0,
            last_processed_sequence: HashMap::new(),
            last_flush: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    fn save_state(&mut self) -> Result<(), std::io::Error> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = self.config.state_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let state_json = serde_json::to_string_pretty(&self.state)?;
        fs::write(&self.config.state_path, state_json)?;

        self.state.last_flush = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        debug!("Saved replicator state to {:?}", self.config.state_path);
        Ok(())
    }

    fn parse_wal_line(line: &str) -> Option<WALEntry> {
        let parts: Vec<&str> = line.split('\t').collect();

        // Most operations need at least 5 fields (op, node, seq, timestamp, bucket)
        if parts.len() < 5 {
            return None;
        }

        let operation = parts[0];
        let node_id = parts[1].to_string();
        let sequence_id = parts[2].parse().ok()?;
        let timestamp = parts[3].parse().ok()?;
        let bucket = parts[4].to_string();

        // Handle different operations with different field counts
        let (key, size, etag) = match operation {
            "PUT" => {
                if parts.len() < 6 {
                    return None;
                }
                let key = parts[5].to_string();
                let size = if parts.len() > 6 { parts[6].parse().ok() } else { None };
                let etag = if parts.len() > 7 {
                    Some(parts[7].to_string()).filter(|s| !s.is_empty())
                } else {
                    None
                };
                (key, size, etag)
            }
            "DELETE" => {
                if parts.len() < 6 {
                    return None;
                }
                (parts[5].to_string(), None, None)
            }
            "CREATE_BUCKET" | "DELETE_BUCKET" => {
                // These operations don't have a key
                (String::new(), None, None)
            }
            "UPDATE_METADATA" => {
                if parts.len() < 7 {
                    return None;
                }
                // metadata_type is stored in key, content in etag
                let metadata_type = parts[5].to_string();
                let content = if parts.len() > 6 {
                    Some(parts[6].to_string())
                } else {
                    None
                };
                (metadata_type, None, content)
            }
            "DELETE_METADATA" => {
                if parts.len() < 6 {
                    return None;
                }
                // metadata_type is stored in key
                (parts[5].to_string(), None, None)
            }
            _ => (String::new(), None, None),
        };

        Some(WALEntry {
            node_id,
            sequence_id,
            timestamp,
            operation: operation.to_string(),
            bucket,
            key,
            size,
            etag,
        })
    }

    async fn read_wal_entries(&mut self) -> Result<Vec<WALEntry>, std::io::Error> {
        let mut entries = Vec::new();

        // Open WAL file
        let mut file = OpenOptions::new()
            .read(true)
            .open(&self.config.wal_path)?;

        // Seek to last processed position
        file.seek(SeekFrom::Start(self.state.last_processed_position))?;

        let mut reader = BufReader::new(&file);
        let mut line = String::new();
        let mut bytes_read = 0;

        while reader.read_line(&mut line)? > 0 {
            if let Some(entry) = Self::parse_wal_line(line.trim()) {
                // Only process entries from our own node (for reading from WAL)
                if entry.node_id == self.config.node_id {
                    // Check if we've already processed this sequence
                    let last_seq = self.state.last_processed_sequence
                        .get(&entry.node_id)
                        .copied()
                        .unwrap_or(0);

                    if entry.sequence_id > last_seq {
                        entries.push(entry.clone());
                        self.state.last_processed_sequence
                            .insert(entry.node_id.clone(), entry.sequence_id);
                    }
                }
            }

            bytes_read += line.len() as u64;
            line.clear();

            // Stop if we've read enough entries
            if entries.len() >= self.config.max_batch_size {
                break;
            }
        }

        // Update position
        self.state.last_processed_position += bytes_read;

        Ok(entries)
    }

    async fn process_batch(&mut self, entries: Vec<WALEntry>) -> Result<(), Box<dyn std::error::Error>> {
        if entries.is_empty() {
            return Ok(());
        }

        info!("Processing batch of {} entries", entries.len());

        // Analyze batch for optimization
        let optimized_entries = self.optimize_batch(entries);

        // Broadcast to other nodes
        for node_address in &self.config.cluster_nodes {
            if let Err(e) = self.send_to_node(node_address, &optimized_entries).await {
                warn!("Failed to send batch to {}: {}", node_address, e);
            }
        }

        // Save state after successful processing
        if let Err(e) = self.save_state() {
            error!("Failed to save state: {}", e);
        }

        Ok(())
    }

    fn optimize_batch(&self, entries: Vec<WALEntry>) -> Vec<WALEntry> {
        // Group operations by (bucket, key)
        let mut operations: HashMap<(String, String), Vec<WALEntry>> = HashMap::new();

        for entry in entries {
            let key = (entry.bucket.clone(), entry.key.clone());
            operations.entry(key).or_default().push(entry);
        }

        // Filter out create/delete pairs
        let mut optimized = Vec::new();

        for ((bucket, key), ops) in operations {
            let has_create = ops.iter().any(|e| e.operation == "PUT");
            let has_delete = ops.iter().any(|e| e.operation == "DELETE");

            if has_create && has_delete {
                info!("Skipping replication for {}/{} - created and deleted in same batch", bucket, key);
                continue;
            }

            // Take only the last operation for this key
            if let Some(last_op) = ops.into_iter().last() {
                optimized.push(last_op);
            }
        }

        optimized
    }

    async fn send_to_node(
        &self,
        node_address: &str,
        entries: &[WALEntry],
    ) -> Result<(), Box<dyn std::error::Error>> {
        // For now, since we're running on the same host, we can directly write to the other node's storage
        // In production, this would need proper HTTP API or gRPC

        // Extract node name from address (e.g., "ironbucket-node1:9000" -> "node1")
        let target_node = if node_address.contains("node1") {
            "node1"
        } else if node_address.contains("node2") {
            "node2"
        } else {
            return Err("Unknown target node".into());
        };

        for entry in entries {
            // Build the target path
            let target_storage = PathBuf::from(format!("/cluster-wal/{}/s3", target_node));

            match entry.operation.as_str() {
                "PUT" => {
                    // Copy file from our storage to target storage
                    let source_path = self.config.storage_path
                        .join(&entry.bucket)
                        .join(&entry.key);

                    let target_path = target_storage
                        .join(&entry.bucket)
                        .join(&entry.key);

                    if source_path.exists() {
                        // Create parent directories
                        if let Some(parent) = target_path.parent() {
                            fs::create_dir_all(parent)?;
                        }

                        // Copy the file
                        fs::copy(&source_path, &target_path)?;

                        // Also copy metadata if it exists
                        let source_metadata = PathBuf::from(format!("{}.metadata", source_path.display()));
                        let target_metadata = PathBuf::from(format!("{}.metadata", target_path.display()));
                        if source_metadata.exists() {
                            fs::copy(&source_metadata, &target_metadata)?;
                        }

                        info!("Replicated {}/{} to {}", entry.bucket, entry.key, target_node);
                    }
                }
                "DELETE" => {
                    let target_path = target_storage
                        .join(&entry.bucket)
                        .join(&entry.key);

                    if target_path.exists() {
                        fs::remove_file(&target_path)?;

                        // Also remove metadata
                        let target_metadata = PathBuf::from(format!("{}.metadata", target_path.display()));
                        if target_metadata.exists() {
                            fs::remove_file(&target_metadata)?;
                        }

                        info!("Deleted {}/{} on {}", entry.bucket, entry.key, target_node);
                    }
                }
                "CREATE_BUCKET" => {
                    let target_path = target_storage.join(&entry.bucket);
                    if !target_path.exists() {
                        fs::create_dir_all(&target_path)?;

                        // Create bucket metadata
                        let metadata = serde_json::json!({
                            "created": chrono::Utc::now().to_rfc3339(),
                            "versioning_status": null,
                        });
                        let metadata_path = target_path.join(".bucket_metadata");
                        fs::write(&metadata_path, metadata.to_string())?;

                        info!("Created bucket {} on {}", entry.bucket, target_node);
                    }
                }
                "DELETE_BUCKET" => {
                    let target_path = target_storage.join(&entry.bucket);
                    if target_path.exists() {
                        fs::remove_dir_all(&target_path)?;
                        info!("Deleted bucket {} on {}", entry.bucket, target_node);
                    }
                }
                "UPDATE_METADATA" => {
                    // metadata_type is in entry.key, content is in entry.etag
                    let metadata_type = &entry.key;
                    let content = entry.etag.as_ref().unwrap_or(&String::new()).clone();

                    // Unescape the content
                    let unescaped_content = content.replace("\\n", "\n").replace("\\t", "\t");

                    let target_bucket = target_storage.join(&entry.bucket);
                    if target_bucket.exists() {
                        let metadata_file = target_bucket.join(format!(".{}", metadata_type));
                        fs::write(&metadata_file, &unescaped_content)?;
                        info!("Updated {} metadata for bucket {} on {}", metadata_type, entry.bucket, target_node);
                    }
                }
                "DELETE_METADATA" => {
                    // metadata_type is in entry.key
                    let metadata_type = &entry.key;

                    let target_bucket = target_storage.join(&entry.bucket);
                    if target_bucket.exists() {
                        let metadata_file = target_bucket.join(format!(".{}", metadata_type));
                        if metadata_file.exists() {
                            fs::remove_file(&metadata_file)?;
                            info!("Deleted {} metadata for bucket {} on {}", metadata_type, entry.bucket, target_node);
                        }
                    }
                }
                _ => {
                    warn!("Unknown operation: {}", entry.operation);
                }
            }
        }

        debug!("Successfully replicated {} entries to {}", entries.len(), target_node);
        Ok(())
    }

    async fn handle_incoming_replication(
        &mut self,
        source_node: String,
        entries: Vec<WALEntry>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!("Received {} entries from {}", entries.len(), source_node);

        for entry in entries {
            // Check for duplicates
            let event_id = (entry.node_id.clone(), entry.sequence_id);
            if self.seen_events.contains(&event_id) {
                debug!("Skipping duplicate event: {:?}", event_id);
                continue;
            }

            // Apply the operation directly to disk (no WAL, no API)
            self.apply_operation(&entry).await?;

            // Mark as seen
            self.seen_events.insert(event_id);
        }

        Ok(())
    }

    async fn apply_operation(&self, entry: &WALEntry) -> Result<(), Box<dyn std::error::Error>> {
        match entry.operation.as_str() {
            "PUT" => {
                // Download object from source node
                let data = self.download_object(&entry.node_id, &entry.bucket, &entry.key).await?;

                // Write directly to filesystem - NO WAL, NO API call
                let object_path = self.config.storage_path
                    .join(&entry.bucket)
                    .join(&entry.key);

                // Create parent directories
                if let Some(parent) = object_path.parent() {
                    fs::create_dir_all(parent)?;
                }

                // Write file
                fs::write(&object_path, data)?;

                info!("Replicated {}/{} from node {}", entry.bucket, entry.key, entry.node_id);
            }
            "DELETE" => {
                // Delete directly from filesystem
                let object_path = self.config.storage_path
                    .join(&entry.bucket)
                    .join(&entry.key);

                if object_path.exists() {
                    fs::remove_file(&object_path)?;
                    info!("Deleted {}/{} (replicated from {})", entry.bucket, entry.key, entry.node_id);
                }
            }
            "CREATE_BUCKET" => {
                let bucket_path = self.config.storage_path.join(&entry.bucket);
                if !bucket_path.exists() {
                    fs::create_dir_all(&bucket_path)?;
                    info!("Created bucket {} (replicated from {})", entry.bucket, entry.node_id);
                }
            }
            "DELETE_BUCKET" => {
                let bucket_path = self.config.storage_path.join(&entry.bucket);
                if bucket_path.exists() {
                    fs::remove_dir_all(&bucket_path)?;
                    info!("Deleted bucket {} (replicated from {})", entry.bucket, entry.node_id);
                }
            }
            _ => {
                warn!("Unknown operation: {}", entry.operation);
            }
        }

        Ok(())
    }

    async fn download_object(
        &self,
        node_id: &str,
        bucket: &str,
        key: &str,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Find the node address from node_id
        // For now, we'll use a simple mapping - in production this would be from configuration
        let node_address = self.config.cluster_nodes
            .iter()
            .find(|addr| addr.contains(node_id))
            .ok_or_else(|| format!("Unknown node: {}", node_id))?;

        let url = format!("http://{}/{}/{}", node_address, bucket, key);

        let response = self.http_client
            .get(&url)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("Failed to download object: {}", response.status()).into());
        }

        let data = response.bytes().await?.to_vec();
        Ok(data)
    }

    pub async fn run(&mut self) {
        info!("Starting replicator for node {}", self.config.node_id);
        info!("Cluster nodes: {:?}", self.config.cluster_nodes);
        info!("WAL path: {:?}", self.config.wal_path);
        info!("State path: {:?}", self.config.state_path);
        info!("Batch interval: {}ms", self.config.batch_interval_ms);

        let batch_interval = Duration::from_millis(self.config.batch_interval_ms);
        let mut last_batch_time = Instant::now();

        loop {
            // Read new WAL entries
            match self.read_wal_entries().await {
                Ok(entries) => {
                    if !entries.is_empty() {
                        self.event_buffer.extend(entries);
                    }
                }
                Err(e) => {
                    if self.config.wal_path.exists() {
                        warn!("Failed to read WAL: {}", e);
                    } else {
                        debug!("WAL file not found, waiting...");
                    }
                }
            }

            // Process batch if interval elapsed or buffer is full
            let should_process = !self.event_buffer.is_empty() &&
                (last_batch_time.elapsed() >= batch_interval ||
                 self.event_buffer.len() >= self.config.max_batch_size);

            if should_process {
                let batch = std::mem::take(&mut self.event_buffer);
                if let Err(e) = self.process_batch(batch).await {
                    error!("Failed to process batch: {}", e);
                }
                last_batch_time = Instant::now();
            }

            // Short sleep to prevent busy loop
            sleep(Duration::from_millis(100)).await;
        }
    }
}

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("replicator=debug".parse().unwrap())
        )
        .init();

    info!("IronBucket Replicator starting...");

    let config = ReplicatorConfig::from_env();
    let mut replicator = Replicator::new(config);

    replicator.run().await;
}