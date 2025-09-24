use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{debug, info};

// Function to recursively remove empty directories
pub async fn cleanup_empty_directories(storage_path: PathBuf) {
    let auto_remove = env::var("AUTO_REMOVE_EMPTY_FOLDERS")
        .unwrap_or_else(|_| "0".to_string());

    if auto_remove != "1" {
        info!("Auto-remove empty folders is disabled");
        return;
    }

    let interval_minutes = env::var("AUTO_REMOVE_EMPTY_FOLDERS_EVERY_X_MIN")
        .unwrap_or_else(|_| "5".to_string())
        .parse::<u64>()
        .unwrap_or(5);

    info!("Starting empty folder cleanup task - will run every {} minutes", interval_minutes);

    // Wait for first interval before starting cleanup
    tokio::time::sleep(Duration::from_secs(interval_minutes * 60)).await;

    loop {
        info!("Running empty folder cleanup scan...");
        let mut removed_count = 0;

        // Scan all bucket directories
        if let Ok(entries) = fs::read_dir(&storage_path) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    // This is a bucket directory - never delete it, only clean inside
                    removed_count += remove_empty_dirs_in_bucket(&entry.path());
                }
            }
        }

        if removed_count > 0 {
            info!("Cleanup completed: removed {} empty directories", removed_count);
        } else {
            debug!("Cleanup completed: no empty directories found");
        }

        // Wait for the next interval
        tokio::time::sleep(Duration::from_secs(interval_minutes * 60)).await;
    }
}

// Helper function to remove empty subdirectories within a bucket (never the bucket itself)
pub fn remove_empty_dirs_in_bucket(bucket_dir: &std::path::Path) -> usize {
    let mut removed_count = 0;

    if let Ok(entries) = fs::read_dir(bucket_dir) {
        let subdirs: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();

        // Process each subdirectory
        for subdir in &subdirs {
            removed_count += remove_empty_subdir_recursive(&subdir.path());
        }
    }

    removed_count
}

// Recursively remove empty subdirectories (used for directories inside buckets)
pub fn remove_empty_subdir_recursive(dir: &std::path::Path) -> usize {
    let mut removed_count = 0;

    // First, recursively process all subdirectories
    if let Ok(entries) = fs::read_dir(dir) {
        let subdirs: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();

        // Recursively clean subdirectories first
        for subdir in &subdirs {
            removed_count += remove_empty_subdir_recursive(&subdir.path());
        }
    }

    // Now check if this directory is empty and can be removed
    // Don't remove .multipart directories as they may be needed
    if dir.file_name() != Some(std::ffi::OsStr::new(".multipart")) {
        if let Ok(mut entries) = fs::read_dir(dir) {
            if entries.next().is_none() {
                // Directory is empty
                if fs::remove_dir(dir).is_ok() {
                    debug!("Removed empty directory: {:?}", dir);
                    removed_count += 1;
                }
            }
        }
    }

    removed_count
}