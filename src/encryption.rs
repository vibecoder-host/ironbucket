use crate::error::Result;
use ring::{aead, error, rand};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use std::fs;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EncryptionType {
    None,
    AES256,
    AWSKMS,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketEncryptionConfig {
    pub encryption_type: EncryptionType,
    pub kms_key_id: Option<String>,
}

pub struct EncryptionManager {
    master_key: Option<Vec<u8>>,
    bucket_configs: Arc<Mutex<HashMap<String, BucketEncryptionConfig>>>,
    storage_path: PathBuf,
}

impl EncryptionManager {
    pub fn new(storage_path: PathBuf) -> Self {
        // Check for environment variable for master key
        let master_key = std::env::var("ENCRYPTION_KEY")
            .ok()
            .and_then(|key| BASE64.decode(&key).ok())
            .or_else(|| {
                // Generate a random master key if not provided
                if std::env::var("ENABLE_ENCRYPTION").unwrap_or_default() == "true" {
                    let rng = rand::SystemRandom::new();
                    let mut key = vec![0u8; 32];
                    if rand::SecureRandom::fill(&rng, &mut key).is_ok() {
                        Some(key)
                    } else {
                        None
                    }
                } else {
                    None
                }
            });

        Self {
            master_key,
            bucket_configs: Arc::new(Mutex::new(HashMap::new())),
            storage_path,
        }
    }

    pub async fn encrypt(&self, data: &[u8], encryption_key: Option<&[u8]>) -> Result<(Vec<u8>, Vec<u8>)> {
        // Use provided key or generate a new one
        let key = if let Some(k) = encryption_key {
            k.to_vec()
        } else if let Some(ref master) = self.master_key {
            master.clone()
        } else {
            // Generate a new key
            let rng = rand::SystemRandom::new();
            let mut key = vec![0u8; 32];
            rand::SecureRandom::fill(&rng, &mut key)
                .map_err(|e| anyhow::anyhow!("Failed to generate key: {:?}", e))?;
            key
        };

        // Create AES-256-GCM cipher
        let unbound_key = aead::UnboundKey::new(&aead::AES_256_GCM, &key)
            .map_err(|e| anyhow::anyhow!("Failed to create unbound key: {:?}", e))?;
        let key = aead::LessSafeKey::new(unbound_key);

        // Generate nonce
        let rng = rand::SystemRandom::new();
        let mut nonce_bytes = [0u8; 12]; // 96 bits for GCM
        rand::SecureRandom::fill(&rng, &mut nonce_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to generate nonce: {:?}", e))?;
        let nonce = aead::Nonce::assume_unique_for_key(nonce_bytes);

        // Encrypt data
        let mut in_out = data.to_vec();
        key.seal_in_place_append_tag(nonce, aead::Aad::empty(), &mut in_out)
            .map_err(|e| anyhow::anyhow!("Encryption failed: {:?}", e))?;

        Ok((in_out, nonce_bytes.to_vec()))
    }

    pub async fn decrypt(&self, ciphertext: &[u8], nonce: &[u8], encryption_key: &[u8]) -> Result<Vec<u8>> {
        // Create AES-256-GCM cipher
        let unbound_key = aead::UnboundKey::new(&aead::AES_256_GCM, encryption_key)
            .map_err(|e| anyhow::anyhow!("Failed to create unbound key: {:?}", e))?;
        let key = aead::LessSafeKey::new(unbound_key);

        // Create nonce
        if nonce.len() != 12 {
            return Err(anyhow::anyhow!("Invalid nonce length: expected 12, got {}", nonce.len()));
        }
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes.copy_from_slice(nonce);
        let nonce = aead::Nonce::assume_unique_for_key(nonce_bytes);

        // Decrypt data
        let mut in_out = ciphertext.to_vec();
        let plaintext = key.open_in_place(nonce, aead::Aad::empty(), &mut in_out)
            .map_err(|e| anyhow::anyhow!("Decryption failed: {:?}", e))?;

        Ok(plaintext.to_vec())
    }

    pub async fn set_bucket_encryption(&self, bucket: &str, encryption_type: EncryptionType, kms_key_id: Option<String>) -> Result<()> {
        let config = BucketEncryptionConfig {
            encryption_type: encryption_type.clone(),
            kms_key_id,
        };

        // Store in memory
        {
            let mut configs = self.bucket_configs.lock().unwrap();
            configs.insert(bucket.to_string(), config.clone());
        }

        // Persist to disk
        let encryption_file = self.storage_path.join(bucket).join(".encryption_config");
        if let Ok(config_json) = serde_json::to_string(&config) {
            fs::create_dir_all(self.storage_path.join(bucket))?;
            fs::write(&encryption_file, config_json)?;
        }

        Ok(())
    }

    pub async fn get_bucket_encryption(&self, bucket: &str) -> Result<EncryptionType> {
        // Check memory first
        {
            let configs = self.bucket_configs.lock().unwrap();
            if let Some(config) = configs.get(bucket) {
                return Ok(config.encryption_type.clone());
            }
        }

        // Check disk
        let encryption_file = self.storage_path.join(bucket).join(".encryption_config");
        if encryption_file.exists() {
            if let Ok(config_json) = fs::read_to_string(&encryption_file) {
                if let Ok(config) = serde_json::from_str::<BucketEncryptionConfig>(&config_json) {
                    // Cache in memory
                    let mut configs = self.bucket_configs.lock().unwrap();
                    configs.insert(bucket.to_string(), config.clone());
                    return Ok(config.encryption_type);
                }
            }
        }

        Ok(EncryptionType::None)
    }
}