use std::path::{Path, PathBuf};

/// Shared layout for application-owned library data.
pub struct LibraryDirs {
    pub root: PathBuf,
}

impl LibraryDirs {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }
    /// Hidden application data root: <root>/.clipstudio-open.
    pub fn app(&self) -> PathBuf {
        self.root.join(".clipstudio-open")
    }
    pub fn db(&self) -> PathBuf {
        self.app().join("library.db")
    }
    pub fn images(&self) -> PathBuf {
        self.root.join("images")
    }
    /// Derived image layout: images/YY-MM/<asset-UUIDv7>/.
    pub fn asset(&self, imported_at: i64, asset_uid: &str) -> PathBuf {
        let dt = chrono::DateTime::from_timestamp(imported_at, 0).unwrap_or_default();
        self.images()
            .join(dt.format("%y-%m").to_string())
            .join(asset_uid)
    }
    pub fn thumbnail(&self, imported_at: i64, asset_uid: &str, source_name: &str) -> PathBuf {
        let stem = Path::new(source_name)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        self.asset(imported_at, asset_uid)
            .join(format!("{stem}_thumbnail.webp"))
    }
    pub fn asset_keyframes(&self, imported_at: i64, asset_uid: &str) -> PathBuf {
        self.asset(imported_at, asset_uid).join("keyframes")
    }
    /// Managed trash storage; referenced sources remain outside the library.
    pub fn trash(&self) -> PathBuf {
        self.app().join("trash")
    }
    /// Vector-index storage.
    pub fn vector(&self) -> PathBuf {
        self.app().join("vector")
    }
}
