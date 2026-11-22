use serde::{Serialize, Serializer};

/// Shared command errors serialize as {code, message} so the client can localize and handle
/// them.
#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("{0}")]
    Msg(String),

    /// The requested library is absent or not open.
    #[error("Library does not exist or is not open")]
    LibraryNotOpen,
    #[error("Library does not exist")]
    LibraryNotFound,

    /// The library directory is busy or already contains data.
    #[error("{0}")]
    LibraryState(String),

    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Asset storage quota exceeded")]
    QuotaExceeded {
        used_bytes: u64,
        incoming_bytes: u64,
        limit_bytes: u64,
    },

    #[error("Insufficient disk space: need {incoming_bytes} bytes, only {available_bytes} bytes available")]
    DiskSpaceInsufficient {
        incoming_bytes: u64,
        available_bytes: u64,
    },
    #[error("Whisper requires {required_bytes} bytes of available memory. {available_bytes} bytes are available.")]
    AiSubtitleMemoryInsufficient {
        required_bytes: u64,
        available_bytes: u64,
    },
    #[error("first import totals {incoming_bytes} bytes; maximum is {limit_bytes} bytes")]
    FirstImportSizeExceeded {
        incoming_bytes: u64,
        limit_bytes: u64,
    },
    #[error("A library operation is running; wait for it to finish and retry")]
    OperationBusy,
    #[error("MCP service is disabled")]
    McpDisabled,
    #[error("MCP write access is disabled")]
    McpWriteDisabled,
    #[error("Unauthorized MCP request")]
    Unauthorized,
    #[error("Asset not found")]
    AssetNotFound,
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
    #[error("This operation is unsupported for this media kind")]
    AssetTypeUnsupported,
    #[error("Invalid video time range")]
    InvalidTimeRange,
    #[error("Derived file generation failed: {0}")]
    DerivativeFailed(String),
    #[error("File access grant expired")]
    DownloadExpired,
    #[error("File access grant not found")]
    DownloadNotFound,
    #[error("Too many requests")]
    RateLimited,
    #[error("Too many derived tasks; retry later")]
    TooManyOperations,

    /// Stable codes drive client localization; message contains English diagnostics for logs
    /// and fallback display.
    #[error("{message}")]
    Coded { code: &'static str, message: String },
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 6)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("message", &self.to_string())?;
        match self {
            AppError::QuotaExceeded {
                used_bytes,
                incoming_bytes,
                limit_bytes,
            } => {
                s.serialize_field("usedBytes", used_bytes)?;
                s.serialize_field("incomingBytes", incoming_bytes)?;
                s.serialize_field("limitBytes", limit_bytes)?;
                s.serialize_field("remainingBytes", &limit_bytes.saturating_sub(*used_bytes))?;
            }
            AppError::DiskSpaceInsufficient {
                incoming_bytes,
                available_bytes,
            } => {
                s.serialize_field("incomingBytes", incoming_bytes)?;
                s.serialize_field("availableBytes", available_bytes)?;
            }
            AppError::AiSubtitleMemoryInsufficient {
                required_bytes,
                available_bytes,
            } => {
                s.serialize_field("requiredBytes", required_bytes)?;
                s.serialize_field("availableBytes", available_bytes)?;
            }
            AppError::FirstImportSizeExceeded {
                incoming_bytes,
                limit_bytes,
            } => {
                s.serialize_field("incomingBytes", incoming_bytes)?;
                s.serialize_field("limitBytes", limit_bytes)?;
            }
            AppError::OperationBusy => {}
            AppError::McpDisabled
            | AppError::McpWriteDisabled
            | AppError::Unauthorized
            | AppError::AssetNotFound
            | AppError::InvalidArgument(_)
            | AppError::AssetTypeUnsupported
            | AppError::InvalidTimeRange
            | AppError::DerivativeFailed(_)
            | AppError::DownloadExpired
            | AppError::DownloadNotFound
            | AppError::RateLimited
            | AppError::TooManyOperations => {}
            AppError::LibraryNotFound => {}
            _ => {}
        }
        s.end()
    }
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Msg(_) => "generic",
            AppError::LibraryNotOpen => "library_not_open",
            AppError::LibraryNotFound => "library_not_found",
            AppError::LibraryState(_) => "library_state",
            AppError::Db(_) => "db",
            AppError::Io(_) => "io",
            AppError::Json(_) => "json",
            AppError::QuotaExceeded { .. } => "quota_exceeded",
            AppError::DiskSpaceInsufficient { .. } => "disk_space_insufficient",
            AppError::AiSubtitleMemoryInsufficient { .. } => "ai_subtitle_memory_insufficient",
            AppError::FirstImportSizeExceeded { .. } => "first_import_size_exceeded",
            AppError::OperationBusy => "operation_busy",
            AppError::McpDisabled => "mcp_disabled",
            AppError::McpWriteDisabled => "write_disabled",
            AppError::Unauthorized => "unauthorized",
            AppError::AssetNotFound => "asset_not_found",
            AppError::InvalidArgument(_) => "invalid_argument",
            AppError::AssetTypeUnsupported => "asset_type_unsupported",
            AppError::InvalidTimeRange => "invalid_time_range",
            AppError::DerivativeFailed(_) => "derivative_failed",
            AppError::DownloadExpired => "download_expired",
            AppError::DownloadNotFound => "download_not_found",
            AppError::RateLimited => "rate_limited",
            AppError::TooManyOperations => "too_many_operations",
            AppError::Coded { code, .. } => code,
        }
    }

    /// Construct a coded error with English diagnostic details for logs and untranslated
    /// fallbacks.
    pub fn coded(code: &'static str, message: impl Into<String>) -> Self {
        AppError::Coded {
            code,
            message: message.into(),
        }
    }
}

impl From<String> for AppError {
    fn from(v: String) -> Self {
        AppError::Msg(v)
    }
}
impl From<&str> for AppError {
    fn from(v: &str) -> Self {
        AppError::Msg(v.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
