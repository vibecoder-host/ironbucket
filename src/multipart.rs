use crate::{config::Config, error::Result, storage::StorageBackend};
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

pub struct MultipartUpload {
    pub upload_id: String,
    pub bucket: String,
    pub key: String,
    pub parts: HashMap<u32, Part>,
    pub metadata: HashMap<String, String>,
}

pub struct Part {
    pub part_number: u32,
    pub etag: String,
    pub size: u64,
    pub data: Vec<u8>,
}

pub struct MultipartManager {
    storage: Arc<dyn StorageBackend>,
}

impl MultipartManager {
    pub async fn new(config: &Config, storage: Arc<dyn StorageBackend>) -> Result<Self> {
        Ok(Self { storage })
    }

    pub async fn create_upload(
        &self,
        bucket: &str,
        key: &str,
        metadata: HashMap<String, String>,
    ) -> Result<String> {
        let upload_id = Uuid::new_v4().to_string();
        // TODO: Store upload info
        Ok(upload_id)
    }

    pub async fn upload_part(
        &self,
        upload_id: &str,
        part_number: u32,
        data: Vec<u8>,
    ) -> Result<String> {
        let etag = format!("{:x}", md5::compute(&data));
        // TODO: Store part
        Ok(etag)
    }

    pub async fn complete_upload(
        &self,
        upload_id: &str,
        parts: Vec<(u32, String)>,
    ) -> Result<String> {
        // TODO: Assemble parts and create final object
        Ok(String::new())
    }

    pub async fn abort_upload(&self, upload_id: &str) -> Result<()> {
        // TODO: Clean up parts
        Ok(())
    }
}