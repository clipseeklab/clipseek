//! Bounded image responses. Original videos are served by the playback HTTP service.
use crate::{
    error::{AppError, AppResult},
    import::scanner::{self, Kind},
};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
use tauri::http::{Method, Request, Response, StatusCode};

pub(crate) const MAX_PREVIEW_BYTES: u64 = 64 * 1024 * 1024;

pub(crate) fn asset_path(
    kind: &str,
    thumbnail: Option<String>,
    source: String,
) -> AppResult<String> {
    thumbnail
        .or_else(|| (kind != "video").then_some(source))
        .ok_or_else(|| AppError::coded("preview_unavailable", "Video thumbnail is not available"))
}

fn open_image(path: &Path) -> AppResult<File> {
    if !matches!(scanner::classify(path), Some(Kind::Image | Kind::Gif)) {
        return Err(AppError::coded(
            "preview_type_unsupported",
            "Preview requires an image; use the playback service for videos",
        ));
    }
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(AppError::coded(
            "preview_type_unsupported",
            "Preview requires a regular image file",
        ));
    }
    check_size(metadata.len())?;
    Ok(file)
}

pub(crate) fn check_size(length: u64) -> AppResult<()> {
    if length > MAX_PREVIEW_BYTES {
        return Err(AppError::coded(
            "preview_too_large",
            "Image preview exceeds the 64 MiB limit",
        ));
    }
    Ok(())
}

pub(crate) fn read_image(path: &Path) -> AppResult<Vec<u8>> {
    read_bounded(open_image(path)?)
}

fn read_bounded(reader: impl Read) -> AppResult<Vec<u8>> {
    let mut bytes = Vec::new();
    // Bound the read as well as metadata: the file may grow after it is opened.
    reader.take(MAX_PREVIEW_BYTES + 1).read_to_end(&mut bytes)?;
    check_size(bytes.len() as u64)?;
    Ok(bytes)
}

pub(crate) fn response(
    request: Request<Vec<u8>>,
    allowed: impl Fn(&Path) -> bool,
) -> Response<Vec<u8>> {
    let reply = |status: StatusCode, body: Vec<u8>| {
        Response::builder()
            .status(status)
            .header("access-control-allow-origin", "*")
            .header("content-type", "text/plain")
            .body(body)
            .unwrap()
    };
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return reply(StatusCode::METHOD_NOT_ALLOWED, Vec::new());
    }
    let encoded_path = request
        .uri()
        .path()
        .strip_prefix('/')
        .unwrap_or(request.uri().path());
    let Ok(decoded) = percent_encoding::percent_decode_str(encoded_path).decode_utf8() else {
        return reply(StatusCode::BAD_REQUEST, Vec::new());
    };
    let path = PathBuf::from(decoded.as_ref());
    if tauri::path::SafePathBuf::new(path.clone()).is_err() || !allowed(&path) {
        return reply(StatusCode::FORBIDDEN, Vec::new());
    }
    let result = (|| -> AppResult<Response<Vec<u8>>> {
        let _reading = crate::library::activity::work_for_path(&path)?;
        let file = open_image(&path)?;
        let head = request.method() == Method::HEAD;
        let length = file.metadata()?.len();
        let bytes = if head {
            Vec::new()
        } else {
            read_bounded(file)?
        };
        Ok(Response::builder()
            .header("access-control-allow-origin", "*")
            .header("content-type", scanner::mime_of(&path))
            .header(
                "content-length",
                if head { length } else { bytes.len() as u64 },
            )
            .header("x-content-type-options", "nosniff")
            .body(bytes)
            .unwrap())
    })();
    result.unwrap_or_else(|error| {
        let status = match &error {
            AppError::Io(e) if e.kind() == std::io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
            AppError::Io(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                StatusCode::FORBIDDEN
            }
            AppError::OperationBusy => StatusCode::CONFLICT,
            _ if error.code() == "preview_too_large" => StatusCode::PAYLOAD_TOO_LARGE,
            _ if error.code() == "preview_type_unsupported" => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        reply(
            status,
            if request.method() == Method::HEAD {
                Vec::new()
            } else {
                error.code().as_bytes().to_vec()
            },
        )
    })
}

#[cfg(test)]
#[path = "../tests/unit/preview.rs"]
mod tests;
