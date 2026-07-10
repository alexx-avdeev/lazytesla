use thiserror::Error;

#[derive(Debug, Error)]
pub enum VehicleCommandError {
    #[error("invalid private key: {0}")]
    InvalidKey(String),

    #[error("cryptographic error: {0}")]
    Crypto(String),

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("vehicle command fault: {0}")]
    VehicleFault(String),

    #[error("key not paired with vehicle")]
    KeyNotPaired,

    #[error("vehicle unavailable: {0}")]
    VehicleUnavailable(String),

    #[error("fleet API error ({status}): {body}")]
    FleetApi { status: u16, body: String },

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("protobuf error: {0}")]
    Protobuf(#[from] prost::DecodeError),

    #[error("protobuf encode error: {0}")]
    ProtobufEncode(#[from] prost::EncodeError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, VehicleCommandError>;