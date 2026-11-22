//! Real-model verification through the retained import/session/vector/search pipeline.
use clip_studio_lib::{
    clip::ClipEncoder,
    db,
    embedding::domain::{Precision, Settings},
    import::{self, ImportMode},
    library, services, vector,
};
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

#[test]
#[ignore = "requires CLIPSTUDIO_MODELS and CLIPSTUDIO_EMBEDDING_FIXTURES; run explicitly"]
fn real_q8_import_search_limits_and_reference_deletion() {
    let fixtures =
        PathBuf::from(std::env::var("CLIPSTUDIO_EMBEDDING_FIXTURES").expect("fixture root"));
    let settings = Settings {
        precision: Precision::Q8,
        model_directory: std::env::var("CLIPSTUDIO_MODELS").expect("model root"),
    };
    let temp = tempfile::tempdir().unwrap();
    let meta = library::init_library(&temp.path().join("library"), "Real pipeline").unwrap();
    let s = Arc::new(library::open_session(meta.clone()).unwrap());
    let sources =
        ["photo.png", "short.mp4", "limit.mp4", "too-long.mp4"].map(|name| fixtures.join(name));
    let originals = sources
        .iter()
        .map(|p| std::fs::read(p).unwrap())
        .collect::<Vec<_>>();
    let result = import::run_import(
        &s,
        &sources,
        None,
        &AtomicBool::new(false),
        ImportMode::Reference,
        &|_, _, _| {},
        &|_| {},
    );
    assert_eq!(result.failed, 1);
    assert_eq!(result.done, 3, "{result:?}");
    let assets = db::assets::query_assets(
        &s.read_db(),
        &s.root,
        &db::model::AssetFilter {
            page_size: 40,
            ..Default::default()
        },
    )
    .unwrap()
    .items;
    assert_eq!(assets.len(), 3);
    for asset in &assets {
        assert_eq!(asset.storage, "reference");
        assert_eq!(
            s.read_db()
                .query_row(
                    "SELECT COUNT(*) FROM frames WHERE asset_id=?",
                    [asset.id],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
    }
    let encoder = ClipEncoder::load_settings(&settings).unwrap();
    assert_eq!(vector::embed_pending(&s, &encoder, &|_| {}).unwrap(), 3);
    vector::optimize_index(&s).unwrap();
    let query = encoder
        .encode_texts(&["a solid red image"])
        .unwrap()
        .remove(0);
    assert_eq!(query.len(), 768);
    let hits = services::search::run_search_session(&s, query.clone(), None, None, None).unwrap();
    assert_eq!(hits.total, 3);
    assert_eq!(hits.items[0].asset.file_name, "photo.png");
    assert!(hits.items.iter().all(|h| h.timestamp_ms == 0));
    let photo = assets.iter().find(|a| a.file_name == "photo.png").unwrap();
    services::assets::trash_assets(&s, &[photo.id]).unwrap();
    let remaining = services::search::run_search_session(&s, query, None, None, None).unwrap();
    assert_eq!(remaining.total, 2);
    assert!(remaining.items.iter().all(|h| h.asset.id != photo.id));
    for (path, bytes) in sources.iter().zip(originals) {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    drop(s);
    let reopened = library::open_session(meta).unwrap();
    assert_eq!(
        db::assets::query_assets(&reopened.read_db(), &reopened.root, &Default::default())
            .unwrap()
            .total,
        2
    );
}

#[test]
#[ignore = "requires all precision model files and CLIPSTUDIO_EMBEDDING_FIXTURES; run explicitly"]
fn q4_and_fp32_adapters_produce_normalized_text_and_image_vectors() {
    let root = std::env::var("CLIPSTUDIO_MODELS").expect("model root");
    let source = PathBuf::from(std::env::var("CLIPSTUDIO_EMBEDDING_FIXTURES").expect("fixtures"))
        .join("photo.png");
    for precision in [Precision::Q4, Precision::Fp32] {
        let encoder = ClipEncoder::load_settings(&Settings {
            precision,
            model_directory: root.clone(),
        })
        .unwrap();
        let query = encoder.encode_texts(&["a red image"]).unwrap().remove(0);
        let image = encoder.encode_asset(&source, false, &|| false).unwrap();
        for v in [&query, &image] {
            assert_eq!(v.len(), 768);
            assert!(v.iter().all(|x| x.is_finite()));
            assert!((v.iter().map(|x| x * x).sum::<f32>() - 1.).abs() < 1e-4);
        }
        assert!(query.iter().zip(image).map(|(a, b)| a * b).sum::<f32>() > 0.);
    }
}
