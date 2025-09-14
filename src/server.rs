use crate::{
    cache::CacheManager,
    cluster::ClusterManager,
    config::Config,
    error::Result,
    multipart::MultipartManager,
    s3::routes,
    storage::StorageBackend,
};
use axum::Router;
use std::{net::SocketAddr, sync::Arc};
use tokio::signal;
use tracing::info;

pub struct AppState {
    pub config: Config,
    pub storage: Arc<dyn StorageBackend>,
    pub cache: Arc<CacheManager>,
    pub multipart: Arc<MultipartManager>,
    pub cluster: Option<Arc<ClusterManager>>,
}

pub async fn run(config: Config) -> Result<()> {
    // Initialize storage backend
    let storage = Arc::new(crate::storage::FileSystemBackend::new(&config.storage)?);

    // Initialize cache manager
    let cache = Arc::new(CacheManager::new(&config).await?);

    // Initialize multipart manager
    let multipart = Arc::new(MultipartManager::new(&config, storage.clone()).await?);

    // Initialize cluster manager if enabled
    let cluster = if config.cluster.enabled {
        Some(Arc::new(ClusterManager::new(&config).await?))
    } else {
        None
    };

    // Create application state
    let state = Arc::new(AppState {
        config: config.clone(),
        storage,
        cache,
        multipart,
        cluster,
    });

    // Build the application
    let app = build_app(state.clone());

    // Create socket address
    let addr = SocketAddr::from(([0, 0, 0, 0], config.server.port));

    info!("IronBucket listening on {}", addr);

    // Create the server
    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Run the server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn build_app(state: Arc<AppState>) -> Router {
    // Create S3 routes
    let s3_routes = routes::create_routes(state.clone());

    // Build the main application - simplified without complex middleware
    Router::new()
        .nest("/", s3_routes)
        .with_state(state)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, shutting down...");
        },
        _ = terminate => {
            info!("Received terminate signal, shutting down...");
        },
    }
}