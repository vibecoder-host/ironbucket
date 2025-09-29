use crossbeam::channel::{bounded, Sender, TryRecvError};
use std::fs::{self, OpenOptions};
use std::io::{BufReader, BufRead, BufWriter, Write, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::thread;
use tracing::{error, info, debug};

#[derive(Debug)]
pub enum WALOp {
    Put {
        bucket: String,
        key: String,
        size: u64,
        etag: Option<String>,
    },
    Delete {
        bucket: String,
        key: String
    },
    CreateBucket {
        bucket: String,
    },
    DeleteBucket {
        bucket: String,
    },
}

pub struct WALWriter {
    sender: Sender<WALOp>,
    sequence: Arc<AtomicU64>,
    node_id: String,
    enabled: bool,
}

impl WALWriter {
    pub fn new(path: PathBuf, node_id: String, enabled: bool) -> Self {
        if !enabled {
            let (sender, _) = bounded(1);
            return WALWriter {
                sender,
                sequence: Arc::new(AtomicU64::new(0)),
                node_id,
                enabled: false,
            };
        }

        let (sender, receiver) = bounded(10000);

        let writer_node_id = node_id.clone();

        // Load the last sequence number from the WAL file if it exists
        let initial_sequence = Self::load_last_sequence(&path, &node_id).unwrap_or(0);
        info!("Starting WAL writer with sequence: {}", initial_sequence);

        let sequence_counter = Arc::new(AtomicU64::new(initial_sequence));
        let thread_counter = sequence_counter.clone();
        let wal_path = path.clone();

        thread::spawn(move || {
            let mut file = match OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                Ok(f) => BufWriter::with_capacity(1024 * 1024, f),
                Err(e) => {
                    error!("Failed to open WAL file: {}", e);
                    return;
                }
            };

            let mut batch = Vec::with_capacity(1000);
            let mut last_flush = Instant::now();

            loop {
                let timeout = Duration::from_millis(100);

                match receiver.recv_timeout(timeout) {
                    Ok(op) => {
                        batch.push(op);
                    }
                    Err(_) => {
                        // Timeout - check if we need to flush
                    }
                }

                // Drain any additional messages that are ready
                while batch.len() < 1000 {
                    match receiver.try_recv() {
                        Ok(op) => batch.push(op),
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            info!("WAL writer shutting down");
                            return;
                        }
                    }
                }

                // Flush every 5 seconds OR if batch is large (increased for better performance)
                if !batch.is_empty() && (last_flush.elapsed() >= Duration::from_secs(5) || batch.len() >= 1000) {
                    let timestamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64;

                    for op in batch.drain(..) {
                        let sequence = thread_counter.fetch_add(1, Ordering::Relaxed);

                        let line = match op {
                            WALOp::Put { bucket, key, size, etag } => {
                                format!("PUT\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                                    writer_node_id, sequence, timestamp, bucket, key, size,
                                    etag.unwrap_or_default())
                            }
                            WALOp::Delete { bucket, key } => {
                                format!("DELETE\t{}\t{}\t{}\t{}\t{}\n",
                                    writer_node_id, sequence, timestamp, bucket, key)
                            }
                            WALOp::CreateBucket { bucket } => {
                                format!("CREATE_BUCKET\t{}\t{}\t{}\t{}\n",
                                    writer_node_id, sequence, timestamp, bucket)
                            }
                            WALOp::DeleteBucket { bucket } => {
                                format!("DELETE_BUCKET\t{}\t{}\t{}\t{}\n",
                                    writer_node_id, sequence, timestamp, bucket)
                            }
                        };

                        if let Err(e) = file.write_all(line.as_bytes()) {
                            error!("Failed to write to WAL: {}", e);
                        }
                    }

                    // Write sequence state for faster startup
                    let state_path = wal_path.with_extension("sequence");
                    if let Some(last_seq) = thread_counter.load(Ordering::Relaxed).checked_sub(1) {
                        let _ = fs::write(&state_path, format!("{}", last_seq + 1));
                    }

                    // Don't force flush every batch - let OS handle it for better performance
                    // Only flush on important operations or periodically
                    if batch.len() >= 100 || last_flush.elapsed() >= Duration::from_secs(30) {
                        if let Err(e) = file.flush() {
                            error!("Failed to flush WAL: {}", e);
                        }
                        debug!("WAL batch force flushed");
                    }

                    last_flush = Instant::now();
                }
            }
        });

        WALWriter {
            sender,
            sequence: sequence_counter,
            node_id,
            enabled: true,
        }
    }

    #[inline(always)]
    pub fn log_put(&self, bucket: &str, key: &str, size: u64, etag: Option<String>) {
        if !self.enabled {
            return;
        }

        let _ = self.sender.try_send(WALOp::Put {
            bucket: bucket.to_string(),
            key: key.to_string(),
            size,
            etag,
        });
    }

    #[inline(always)]
    pub fn log_delete(&self, bucket: &str, key: &str) {
        if !self.enabled {
            return;
        }

        let _ = self.sender.try_send(WALOp::Delete {
            bucket: bucket.to_string(),
            key: key.to_string(),
        });
    }

    #[inline(always)]
    pub fn log_create_bucket(&self, bucket: &str) {
        if !self.enabled {
            return;
        }

        let _ = self.sender.try_send(WALOp::CreateBucket {
            bucket: bucket.to_string(),
        });
    }

    #[inline(always)]
    pub fn log_delete_bucket(&self, bucket: &str) {
        if !self.enabled {
            return;
        }

        let _ = self.sender.try_send(WALOp::DeleteBucket {
            bucket: bucket.to_string(),
        });
    }

    /// Load the last sequence number from an existing WAL file
    /// Optimized to read only from the end of the file
    fn load_last_sequence(path: &PathBuf, node_id: &str) -> Option<u64> {
        if !path.exists() {
            return None;
        }

        // Try to read sequence state from a separate file first
        let state_path = path.with_extension("sequence");
        if state_path.exists() {
            if let Ok(contents) = fs::read_to_string(&state_path) {
                if let Ok(seq) = contents.trim().parse::<u64>() {
                    info!("Loaded sequence {} from state file", seq);
                    return Some(seq);
                }
            }
        }

        // Fallback: Read only last 10KB of WAL file to find recent sequences
        let file = fs::File::open(path).ok()?;
        let metadata = file.metadata().ok()?;
        let file_size = metadata.len();

        // Read only the last 10KB or the whole file if smaller
        let read_size = std::cmp::min(10240, file_size);
        let mut reader = BufReader::new(file);

        if file_size > read_size {
            reader.seek(SeekFrom::Start(file_size - read_size)).ok()?;
        }

        let mut max_sequence = 0u64;
        let mut line = String::new();

        // Skip potentially partial first line
        let _ = reader.read_line(&mut line);
        line.clear();

        while reader.read_line(&mut line).ok()? > 0 {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 && parts[1] == node_id {
                if let Ok(seq) = parts[2].parse::<u64>() {
                    max_sequence = max_sequence.max(seq);
                }
            }
            line.clear();
        }

        // Return the next sequence number to use
        if max_sequence > 0 {
            Some(max_sequence + 1)
        } else {
            None
        }
    }
}