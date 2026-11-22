use super::*;
pub(super) struct VideoImportOptions {
    pub is_gif: bool,
    pub visual: bool,
    pub index_only: bool,
}
pub(super) fn import_video(
    session: &Arc<LibrarySession>,
    dirs: &LibraryDirs,
    asset_id: i64,
    src: &std::path::Path,
    on_asset: &dyn Fn(&Asset),
    options: VideoImportOptions,
) -> AppResult<()> {
    let thumb_path = thumbnail_path(session, dirs, asset_id, src)?;
    let (info, decoded) = if options.is_gif {
        let decoded = thumb::open_image_smart(src)?;
        let info = ff::MediaInfo {
            width: Some(decoded.width() as i64),
            height: Some(decoded.height() as i64),
            ..Default::default()
        };
        (info, decoded)
    } else {
        let info = ff::probe(src)?;
        crate::embedding::domain::validate_duration(info.duration_ms.unwrap_or(0) as f64 / 1000.)
            .map_err(|e| AppError::InvalidArgument(e.to_string()))?;
        let decoded = crate::embedding::media::Media {
            ffmpeg: crate::media::ffmpeg_path(),
            ffprobe: crate::model_download::runtime().join(if cfg!(windows) {
                "ffprobe.exe"
            } else {
                "ffprobe"
            }),
        }
        .frame(src, 0., 640, 640, false)
        .map_err(|e| AppError::Msg(e.to_string()))?;
        (info, decoded)
    };
    thumb::encode_dynamic(&thumb_path, &decoded)?;
    let rel = util::to_rel(&session.root, &thumb_path);
    let conn = session.db.lock().unwrap();
    db::assets::update_asset_probe(
        &conn,
        asset_id,
        db::assets::AssetProbe {
            width: info.width,
            height: info.height,
            duration_ms: info.duration_ms,
            color: None,
            thumb_rel: Some(&rel),
        },
    )?;
    if options.visual {
        db::assets::insert_frame(&conn, asset_id, 0, 0, Some(&rel))?;
    }
    drop(conn);
    let _ = options.index_only;
    on_asset_and_fetch(session, asset_id, on_asset)
}
