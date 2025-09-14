use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::fmt;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Access denied")]
    AccessDenied,

    #[error("Bucket already exists")]
    BucketAlreadyExists,

    #[error("Bucket not empty")]
    BucketNotEmpty,

    #[error("Bucket not found")]
    NoSuchBucket,

    #[error("Object not found")]
    NoSuchKey,

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Invalid access key ID")]
    InvalidAccessKeyId,

    #[error("Signature does not match")]
    SignatureDoesNotMatch,

    #[error("Request timeout")]
    RequestTimeout,

    #[error("Internal server error: {0}")]
    InternalError(String),

    #[error("Entity too large")]
    EntityTooLarge,

    #[error("Incomplete body")]
    IncompleteBody,

    #[error("Invalid part")]
    InvalidPart,

    #[error("Invalid part order")]
    InvalidPartOrder,

    #[error("No such upload")]
    NoSuchUpload,

    #[error("Precondition failed")]
    PreconditionFailed,

    #[error("Not implemented")]
    NotImplemented,

    #[error("Service unavailable")]
    ServiceUnavailable,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Serialization(err.to_string())
    }
}

impl Error {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Error::AccessDenied => StatusCode::FORBIDDEN,
            Error::BucketAlreadyExists => StatusCode::CONFLICT,
            Error::BucketNotEmpty => StatusCode::CONFLICT,
            Error::NoSuchBucket => StatusCode::NOT_FOUND,
            Error::NoSuchKey => StatusCode::NOT_FOUND,
            Error::InvalidRequest(_) => StatusCode::BAD_REQUEST,
            Error::InvalidArgument(_) => StatusCode::BAD_REQUEST,
            Error::InvalidAccessKeyId => StatusCode::FORBIDDEN,
            Error::SignatureDoesNotMatch => StatusCode::FORBIDDEN,
            Error::RequestTimeout => StatusCode::REQUEST_TIMEOUT,
            Error::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Error::EntityTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Error::IncompleteBody => StatusCode::BAD_REQUEST,
            Error::InvalidPart => StatusCode::BAD_REQUEST,
            Error::InvalidPartOrder => StatusCode::BAD_REQUEST,
            Error::NoSuchUpload => StatusCode::NOT_FOUND,
            Error::PreconditionFailed => StatusCode::PRECONDITION_FAILED,
            Error::NotImplemented => StatusCode::NOT_IMPLEMENTED,
            Error::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Error::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Error::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Error::Redis(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Error::Serialization(_) => StatusCode::BAD_REQUEST,
            Error::Other(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn error_code(&self) -> &str {
        match self {
            Error::AccessDenied => "AccessDenied",
            Error::BucketAlreadyExists => "BucketAlreadyExists",
            Error::BucketNotEmpty => "BucketNotEmpty",
            Error::NoSuchBucket => "NoSuchBucket",
            Error::NoSuchKey => "NoSuchKey",
            Error::InvalidRequest(_) => "InvalidRequest",
            Error::InvalidArgument(_) => "InvalidArgument",
            Error::InvalidAccessKeyId => "InvalidAccessKeyId",
            Error::SignatureDoesNotMatch => "SignatureDoesNotMatch",
            Error::RequestTimeout => "RequestTimeout",
            Error::InternalError(_) => "InternalServerError",
            Error::EntityTooLarge => "EntityTooLarge",
            Error::IncompleteBody => "IncompleteBody",
            Error::InvalidPart => "InvalidPart",
            Error::InvalidPartOrder => "InvalidPartOrder",
            Error::NoSuchUpload => "NoSuchUpload",
            Error::PreconditionFailed => "PreconditionFailed",
            Error::NotImplemented => "NotImplemented",
            Error::ServiceUnavailable => "ServiceUnavailable",
            _ => "InternalError",
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Error>
    <Code>{}</Code>
    <Message>{}</Message>
    <Resource></Resource>
    <RequestId>{}</RequestId>
</Error>"#,
            self.error_code(),
            self,
            uuid::Uuid::new_v4()
        )
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = self.to_xml();

        Response::builder()
            .status(status)
            .header("Content-Type", "application/xml")
            .body(body.into())
            .unwrap()
    }
}