use axum::{
    extract::DefaultBodyLimit,
    middleware,
    routing::{delete, get, head, post, put},
    Router,
};
use std::{
    collections::HashMap,
    env,
    fs,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tower_http::cors::CorsLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Import modules
mod models;
mod utils;
mod cleanup;
mod policy_check;
mod filesystem;
mod handlers;
mod quota;
mod wal;

// Re-export commonly used items from modules
pub use models::*;
pub use utils::format_http_date;
pub use policy_check::check_policy_permission;
pub use filesystem::*;
use handlers::*;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ironbucket=debug,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting IronBucket S3-compatible server with full API support...");

    // Get storage path from environment variable or use default
    let storage_path = env::var("STORAGE_PATH")
        .unwrap_or_else(|_| "/s3".to_string());
    let storage_path = PathBuf::from(storage_path);
    fs::create_dir_all(&storage_path).unwrap();
    info!("Using storage path: {:?}", storage_path);

    // Load credentials from environment variables (required)
    let access_key = env::var("ACCESS_KEY")
        .expect("ACCESS_KEY environment variable must be set");
    let secret_key = env::var("SECRET_KEY")
        .expect("SECRET_KEY environment variable must be set");

    let mut access_keys = HashMap::new();
    access_keys.insert(access_key.clone(), secret_key.clone());

    info!("Using access key: {}", access_key);

    // Check if quota and stats are enabled (default: disabled)
    let enable_quota = env::var("ENABLE_QUOTA_AND_STATS")
        .unwrap_or_else(|_| "0".to_string()) == "1";

    if enable_quota {
        info!("Quota and stats management is ENABLED");
    } else {
        info!("Quota and stats management is DISABLED");
    }

    let quota_manager = Arc::new(quota::QuotaManager::new(storage_path.clone(), enable_quota));

    // Configure WAL (Write-Ahead Log) for replication
    let enable_wal = env::var("ENABLE_WAL")
        .unwrap_or_else(|_| "false".to_string()) == "true";

    let wal_path = if enable_wal {
        let path = env::var("WAL_PATH")
            .unwrap_or_else(|_| "/wal".to_string());
        let wal_dir = PathBuf::from(&path);
        fs::create_dir_all(&wal_dir).unwrap();
        wal_dir.join("wal.log")
    } else {
        PathBuf::from("/dev/null")
    };

    let node_id = env::var("NODE_ID")
        .unwrap_or_else(|_| "node-1".to_string());

    if enable_wal {
        info!("WAL enabled at {:?} with node_id: {}", wal_path, node_id);
    } else {
        info!("WAL disabled");
    }

    let wal_writer = Arc::new(wal::WALWriter::new(wal_path, node_id, enable_wal));

    let state = AppState {
        storage_path: storage_path.clone(),
        access_keys: Arc::new(access_keys),
        multipart_uploads: Arc::new(Mutex::new(HashMap::new())),
        quota_manager: quota_manager.clone(),
        wal_writer,
    };

    let app = Router::new()
        // Root endpoints
        .route("/", get(list_buckets))
        .route("/", post(handle_root_post))

        // Bucket endpoints with query parameter support
        .route("/:bucket", get(handle_bucket_get))
        .route("/:bucket", put(handle_bucket_put))
        .route("/:bucket", post(handle_bucket_post))
        .route("/:bucket", delete(delete_bucket))
        .route("/:bucket", head(head_bucket))
        .route("/:bucket/", get(handle_bucket_get))
        .route("/:bucket/", put(handle_bucket_put))
        .route("/:bucket/", post(handle_bucket_post))
        .route("/:bucket/", delete(delete_bucket))
        .route("/:bucket/", head(head_bucket))

        // Object endpoints with query parameter support
        .route("/:bucket/*key", get(handle_object_get))
        .route("/:bucket/*key", put(handle_object_put))
        // .route("/:bucket/*key", post(handle_object_post))  // TODO: Fix handler compilation
        .route("/:bucket/*key", delete(handle_object_delete))
        .route("/:bucket/*key", head(head_object))

        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .layer(CorsLayer::permissive())
        .layer(DefaultBodyLimit::disable()) // Disable body limit for S3 compatibility
        .with_state(state);

    // Spawn the background cleanup task
    tokio::spawn(cleanup::cleanup_empty_directories(storage_path.clone()));

    // Spawn the quota flush task
    tokio::spawn(quota_manager.start_flush_task());

    let addr = SocketAddr::from(([0, 0, 0, 0], 9000));
    info!("IronBucket listening on {} with full S3 API support", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}