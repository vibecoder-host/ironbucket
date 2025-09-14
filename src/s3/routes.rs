use crate::server::AppState;
use axum::{
    extract::{Path, State},
    routing::{delete, get, head, post, put},
    Router,
};
use std::sync::Arc;

pub fn create_routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        // Service operations
        .route("/", get(super::handlers::list_buckets))
        .route("/health", get(super::handlers::health_check))

        // Bucket operations
        .route("/:bucket", put(super::handlers::create_bucket))
        .route("/:bucket", delete(super::handlers::delete_bucket))
        .route("/:bucket", head(super::handlers::head_bucket))
        .route("/:bucket", get(super::handlers::list_objects))
        .route("/:bucket", post(super::handlers::bucket_operations))

        // Object operations
        .route("/:bucket/*key", put(super::handlers::put_object))
        .route("/:bucket/*key", get(super::handlers::get_object))
        .route("/:bucket/*key", delete(super::handlers::delete_object))
        .route("/:bucket/*key", head(super::handlers::head_object))
        .route("/:bucket/*key", post(super::handlers::object_operations))
}