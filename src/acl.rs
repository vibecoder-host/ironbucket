use crate::error::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessControlList {
    pub owner: Owner,
    pub grants: Vec<Grant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Owner {
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Grant {
    pub grantee: Grantee,
    pub permission: Permission,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Grantee {
    CanonicalUser { id: String, display_name: String },
    Group(String),
    Email(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Permission {
    FullControl,
    Write,
    WriteAcp,
    Read,
    ReadAcp,
}

pub struct AclManager;

impl AclManager {
    pub fn new() -> Self {
        Self
    }

    pub async fn set_acl(&self, bucket: &str, key: Option<&str>, acl: AccessControlList) -> Result<()> {
        // TODO: Store ACL
        Ok(())
    }

    pub async fn get_acl(&self, bucket: &str, key: Option<&str>) -> Result<AccessControlList> {
        // TODO: Retrieve ACL
        Ok(AccessControlList {
            owner: Owner {
                id: "ironbucket".to_string(),
                display_name: "IronBucket User".to_string(),
            },
            grants: vec![],
        })
    }

    pub async fn check_access(
        &self,
        bucket: &str,
        key: Option<&str>,
        user: &str,
        permission: Permission,
    ) -> Result<bool> {
        // TODO: Check if user has permission
        Ok(true) // Allow all for now
    }
}