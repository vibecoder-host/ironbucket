use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub server: ServerConfig,
    pub storage: StorageConfig,
    pub auth: AuthConfig,
    pub redis: RedisConfig,
    pub cluster: ClusterConfig,
    pub performance: PerformanceConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub workers: usize,
    pub max_connections: usize,
    pub request_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageConfig {
    pub path: PathBuf,
    pub max_file_size: u64,
    pub multipart_threshold: u64,
    pub multipart_chunk_size: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub region: String,
    pub signature_version: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RedisConfig {
    pub url: String,
    pub pool_size: usize,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClusterConfig {
    pub enabled: bool,
    pub node_id: String,
    pub peers: Vec<String>,
    pub replication_factor: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PerformanceConfig {
    pub cache_size_mb: usize,
    pub cache_ttl_secs: u64,
    pub io_buffer_size: usize,
    pub compression_enabled: bool,
    pub compression_level: u32,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Config {
            server: ServerConfig {
                host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
                port: env::var("PORT")
                    .unwrap_or_else(|_| "9000".to_string())
                    .parse()?,
                workers: env::var("WORKERS")
                    .unwrap_or_else(|_| num_cpus::get().to_string())
                    .parse()?,
                max_connections: env::var("MAX_CONNECTIONS")
                    .unwrap_or_else(|_| "10000".to_string())
                    .parse()?,
                request_timeout_secs: env::var("REQUEST_TIMEOUT_SECS")
                    .unwrap_or_else(|_| "30".to_string())
                    .parse()?,
            },
            storage: StorageConfig {
                path: PathBuf::from(env::var("STORAGE_PATH").unwrap_or_else(|_| "/s3".to_string())),
                max_file_size: env::var("MAX_FILE_SIZE")
                    .unwrap_or_else(|_| "5368709120".to_string()) // 5GB
                    .parse()?,
                multipart_threshold: env::var("MULTIPART_THRESHOLD")
                    .unwrap_or_else(|_| "104857600".to_string()) // 100MB
                    .parse()?,
                multipart_chunk_size: env::var("MULTIPART_CHUNK_SIZE")
                    .unwrap_or_else(|_| "5242880".to_string()) // 5MB
                    .parse()?,
            },
            auth: AuthConfig {
                access_key_id: env::var("ACCESS_KEY_ID")
                    .unwrap_or_else(|_| "minioadmin".to_string()),
                secret_access_key: env::var("SECRET_ACCESS_KEY")
                    .unwrap_or_else(|_| "minioadmin".to_string()),
                region: env::var("REGION").unwrap_or_else(|_| "us-east-1".to_string()),
                signature_version: env::var("SIGNATURE_VERSION")
                    .unwrap_or_else(|_| "v4".to_string()),
            },
            redis: RedisConfig {
                url: env::var("REDIS_URL")
                    .unwrap_or_else(|_| "redis://172.17.0.1:16379".to_string()),
                pool_size: env::var("REDIS_POOL_SIZE")
                    .unwrap_or_else(|_| "10".to_string())
                    .parse()?,
                timeout_secs: env::var("REDIS_TIMEOUT_SECS")
                    .unwrap_or_else(|_| "5".to_string())
                    .parse()?,
            },
            cluster: ClusterConfig {
                enabled: env::var("CLUSTER_ENABLED")
                    .unwrap_or_else(|_| "false".to_string())
                    .parse()?,
                node_id: env::var("NODE_ID")
                    .unwrap_or_else(|_| uuid::Uuid::new_v4().to_string()),
                peers: env::var("CLUSTER_PEERS")
                    .unwrap_or_else(|_| String::new())
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect(),
                replication_factor: env::var("REPLICATION_FACTOR")
                    .unwrap_or_else(|_| "2".to_string())
                    .parse()?,
            },
            performance: PerformanceConfig {
                cache_size_mb: env::var("CACHE_SIZE_MB")
                    .unwrap_or_else(|_| "256".to_string())
                    .parse()?,
                cache_ttl_secs: env::var("CACHE_TTL_SECS")
                    .unwrap_or_else(|_| "300".to_string())
                    .parse()?,
                io_buffer_size: env::var("IO_BUFFER_SIZE")
                    .unwrap_or_else(|_| "65536".to_string()) // 64KB
                    .parse()?,
                compression_enabled: env::var("COMPRESSION_ENABLED")
                    .unwrap_or_else(|_| "true".to_string())
                    .parse()?,
                compression_level: env::var("COMPRESSION_LEVEL")
                    .unwrap_or_else(|_| "6".to_string())
                    .parse()?,
            },
        })
    }
}

// Helper to get num_cpus
mod num_cpus {
    pub fn get() -> usize {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    }
}