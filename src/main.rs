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

    let state = AppState {
        storage_path: storage_path.clone(),
        access_keys: Arc::new(access_keys),
        multipart_uploads: Arc::new(Mutex::new(HashMap::new())),
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
        .route("/:bucket/*key", post(handle_object_post))
        .route("/:bucket/*key", delete(handle_object_delete))
        .route("/:bucket/*key", head(head_object))

        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .layer(CorsLayer::permissive())
        .layer(DefaultBodyLimit::disable()) // Disable body limit for S3 compatibility
        .with_state(state);

    // Spawn the background cleanup task
    tokio::spawn(cleanup::cleanup_empty_directories(storage_path.clone()));

    let addr = SocketAddr::from(([0, 0, 0, 0], 9000));
    info!("IronBucket listening on {} with full S3 API support", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}