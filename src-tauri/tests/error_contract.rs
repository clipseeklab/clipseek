//! AppError serialization contracts cover stable codes, extra fields, and field allowlists.

use clip_studio_lib::error::AppError;
use serde_json::Value;

/// Every variant must expose a stable code for frontend handling.
#[test]
fn every_variant_maps_to_stable_code() {
    let samples: Vec<(AppError, &str)> = vec![
        (AppError::Msg("x".into()), "generic"),
        (AppError::LibraryNotOpen, "library_not_open"),
        (AppError::LibraryNotFound, "library_not_found"),
        (AppError::LibraryState("x".into()), "library_state"),
        (AppError::Db(rusqlite::Error::InvalidQuery), "db"),
        (AppError::Io(std::io::Error::other("x")), "io"),
        (
            AppError::Json(serde_json::from_str::<Value>("bad").unwrap_err()),
            "json",
        ),
        (
            AppError::QuotaExceeded {
                used_bytes: 0,
                incoming_bytes: 0,
                limit_bytes: 0,
            },
            "quota_exceeded",
        ),
        (
            AppError::DiskSpaceInsufficient {
                incoming_bytes: 0,
                available_bytes: 0,
            },
            "disk_space_insufficient",
        ),
        (
            AppError::FirstImportSizeExceeded {
                incoming_bytes: 101_000_000,
                limit_bytes: 100_000_000,
            },
            "first_import_size_exceeded",
        ),
        (AppError::OperationBusy, "operation_busy"),
        (AppError::McpDisabled, "mcp_disabled"),
        (AppError::McpWriteDisabled, "write_disabled"),
        (AppError::Unauthorized, "unauthorized"),
        (AppError::AssetNotFound, "asset_not_found"),
        (AppError::InvalidArgument("x".into()), "invalid_argument"),
        (AppError::AssetTypeUnsupported, "asset_type_unsupported"),
        (AppError::InvalidTimeRange, "invalid_time_range"),
        (AppError::DerivativeFailed("x".into()), "derivative_failed"),
        (AppError::DownloadExpired, "download_expired"),
        (AppError::DownloadNotFound, "download_not_found"),
        (AppError::RateLimited, "rate_limited"),
        (AppError::TooManyOperations, "too_many_operations"),
    ];
    // Coded errors obtain their code from the payload rather than the enum variant.
    assert_eq!(
        AppError::coded("source_file_missing", "detail").code(),
        "source_file_missing"
    );
    for (err, code) in samples {
        assert_eq!(err.code(), code, "{} should map to {code}", err);
    }
}

#[test]
fn quota_error_serializes_remaining_bytes_for_frontend() {
    let gib = 1024 * 1024 * 1024_u64;
    let err = AppError::QuotaExceeded {
        used_bytes: 4 * gib,
        incoming_bytes: 2 * gib,
        limit_bytes: 5 * gib,
    };
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["code"], "quota_exceeded");
    assert_eq!(json["usedBytes"], 4 * gib);
    assert_eq!(json["incomingBytes"], 2 * gib);
    assert_eq!(json["limitBytes"], 5 * gib);
    assert_eq!(
        json["remainingBytes"], gib,
        "Remaining quota equals limit minus used"
    );

    // Clamp available capacity at zero when usage already exceeds the limit.
    let over = AppError::QuotaExceeded {
        used_bytes: 6 * gib,
        incoming_bytes: 2 * gib,
        limit_bytes: 5 * gib,
    };
    assert_eq!(
        serde_json::to_value(&over).unwrap()["remainingBytes"],
        0,
        "remainingBytes saturates at zero above the limit"
    );
}

#[test]
fn disk_space_error_carries_byte_fields() {
    let err = AppError::DiskSpaceInsufficient {
        incoming_bytes: 100,
        available_bytes: 40,
    };
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["code"], "disk_space_insufficient");
    assert_eq!(json["incomingBytes"], 100);
    assert_eq!(json["availableBytes"], 40);
}

/// Ordinary errors expose only code and message, not internal fields.
#[test]
fn plain_errors_expose_only_code_and_message() {
    let json = serde_json::to_value(AppError::AssetNotFound).unwrap();
    assert_eq!(json["code"], "asset_not_found");
    assert_eq!(json["message"], "Asset not found");
    assert_eq!(json.as_object().unwrap().len(), 2);
}

#[test]
fn strings_convert_into_msg() {
    assert!(matches!(AppError::from("bare".to_string()), AppError::Msg(v) if v == "bare"));
    assert!(matches!(AppError::from("bare"), AppError::Msg(v) if v == "bare"));
}

/// Coded errors forward the payload code and one message without extra fields.
#[test]
fn coded_error_serializes_code_and_message_only() {
    let err = AppError::coded("source_file_missing", "source file is missing: /x/y.png");
    assert_eq!(err.to_string(), "source file is missing: /x/y.png");

    let value = serde_json::to_value(&err).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
            "code": "source_file_missing",
            "message": "source file is missing: /x/y.png",
        })
    );
}

#[test]
fn first_import_size_error_includes_actual_and_limit_bytes() {
    let value = serde_json::to_value(AppError::FirstImportSizeExceeded {
        incoming_bytes: 100_000_001,
        limit_bytes: 100_000_000,
    })
    .unwrap();
    assert_eq!(value["code"], "first_import_size_exceeded");
    assert_eq!(value["incomingBytes"], 100_000_001);
    assert_eq!(value["limitBytes"], 100_000_000);
}
