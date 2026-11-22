use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::AppResult;

/// Normalize Windows extended-length paths for configuration and display.
pub(crate) fn user_friendly_path(path: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{rest}");
        }
        if let Some(rest) = path.strip_prefix(r"\\?\") {
            let bytes = rest.as_bytes();
            if bytes.len() >= 3
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && bytes[2] == b'\\'
            {
                return rest.to_owned();
            }
        }
    }
    path.to_owned()
}

/// Global library registrations and preferences, stored in the application's platform
/// configuration directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GlobalConfig {
    /// Configuration schema version for future migrations.
    pub version: u32,
    /// Registered libraries.
    pub libraries: Vec<LibraryMeta>,
    /// Active library ID.
    pub active_library_id: Option<String>,
    /// User settings.
    pub settings: Settings,
    /// Versioned first-run experience, separate from ordinary settings.
    #[serde(default)]
    pub onboarding: OnboardingState,
    /// Persistent relocation intent recovers exits between the directory switch and
    /// configuration update.
    pub pending_migrations: Vec<PendingMigration>,
    /// The configuration source path is not serialized; tests and embedded uses can inject
    /// isolated locations.
    #[serde(skip)]
    storage_path: Option<PathBuf>,
    #[serde(skip)]
    load_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingMigration {
    pub library_id: String,
    pub old_path: String,
    pub new_path: String,
}

/// One registered library's metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryMeta {
    pub id: String,
    pub name: String,
    /// Absolute library root.
    pub path: String,
    pub created_at: i64,
    /// Optional badge color palette key, such as indigo; legacy configurations may omit it.
    #[serde(default)]
    pub color: Option<String>,
}

/// User settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub mcp_enabled: bool,
    pub mcp_write_enabled: bool,
    pub mcp_port: u16,
    pub mcp_listen_mode: String,
    pub embedding_precision: crate::embedding::domain::Precision,
    pub embedding_model_directory: String,
    /// Interface language: en by default, zh, or auto to follow the system.
    pub language: String,
    /// Theme: auto follows the system; light and dark override it.
    pub theme: String,
    /// Days before automatic trash cleanup.
    pub trash_retention_days: u32,
    /// Import mode is reference-only; source files remain in their original location.
    pub import_mode: String,
    pub auto_visual_index: bool,
    pub auto_subtitle_index: bool,
}

pub const ONBOARDING_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OnboardingState {
    pub version: u32,
    pub current_step: u8,
    pub completed: bool,
}

// Version zero means legacy configuration without persisted onboarding fields.
impl Default for OnboardingState {
    fn default() -> Self {
        Self {
            version: 0,
            current_step: 1,
            completed: false,
        }
    }
}

impl OnboardingState {
    pub fn new_user() -> Self {
        Self {
            version: ONBOARDING_VERSION,
            current_step: 1,
            completed: false,
        }
    }

    pub fn completed() -> Self {
        Self {
            version: ONBOARDING_VERSION,
            current_step: 3,
            completed: true,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mcp_enabled: false,
            mcp_write_enabled: false,
            mcp_port: 0,
            mcp_listen_mode: "loopback".into(),
            embedding_precision: Default::default(),
            embedding_model_directory: String::new(),
            language: "en".into(),
            theme: "auto".into(),
            trash_retention_days: 30,
            import_mode: "reference".into(),
            auto_visual_index: true,
            auto_subtitle_index: false,
        }
    }
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            version: 1,
            libraries: Vec::new(),
            active_library_id: None,
            settings: Settings::default(),
            onboarding: OnboardingState::new_user(),
            pending_migrations: Vec::new(),
            storage_path: None,
            load_error: None,
        }
    }
}

impl GlobalConfig {
    pub(crate) fn load_for_startup() -> Self {
        match Self::path() {
            Ok(path) => Self::startup_from(path),
            Err(error) => Self {
                load_error: Some(error.to_string()),
                ..Self::default()
            },
        }
    }

    fn startup_from(path: PathBuf) -> Self {
        Self::load_from(path.clone()).unwrap_or_else(|error| {
            crate::logging::error("config", "load_failed", "-", &error.to_string());
            Self {
                storage_path: Some(path),
                load_error: Some(error.to_string()),
                ..Self::default()
            }
        })
    }

    /// Retry a failed startup load without publishing an empty configuration.
    pub(crate) fn ensure_loaded(&mut self) -> AppResult<()> {
        if self.load_error.is_some() {
            let path = self
                .storage_path
                .clone()
                .map(Ok)
                .unwrap_or_else(Self::path)?;
            *self = Self::load_from(path).map_err(|error| {
                crate::error::AppError::coded("config_load_failed", error.to_string())
            })?;
        }
        Ok(())
    }

    /// Load configuration or return defaults when the file is absent.
    pub fn load() -> AppResult<Self> {
        Self::load_from(Self::path()?)
    }

    /// Load from an explicit path and retain it for future saves, keeping tests isolated from
    /// user configuration.
    pub fn load_from(path: PathBuf) -> AppResult<Self> {
        let raw = match fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    storage_path: Some(path),
                    ..Self::default()
                });
            }
            Err(error) => return Err(error.into()),
        };
        let mut config: Self = serde_json::from_str(&raw)?;
        // Existing libraries in legacy configurations should not restart onboarding.
        if config.onboarding.version == 0 {
            config.onboarding = if config.libraries.is_empty() {
                OnboardingState::new_user()
            } else {
                OnboardingState::completed()
            };
        } else if config.onboarding.version < ONBOARDING_VERSION {
            // Onboarding v2 removed the first-search step; old step 4 already counts as
            // completed.
            config.onboarding.completed =
                config.onboarding.completed || config.onboarding.current_step >= 4;
            config.onboarding.current_step = config.onboarding.current_step.min(3);
            config.onboarding.version = ONBOARDING_VERSION;
        }
        config.storage_path = Some(path.clone());
        if !config.pending_migrations.is_empty() {
            for pending in &config.pending_migrations {
                let old_exists = PathBuf::from(&pending.old_path).exists();
                let new_exists = PathBuf::from(&pending.new_path).exists();
                if let Some(meta) = config
                    .libraries
                    .iter_mut()
                    .find(|l| l.id == pending.library_id)
                {
                    if new_exists {
                        meta.path = pending.new_path.clone();
                    } else if old_exists {
                        meta.path = pending.old_path.clone();
                    }
                }
            }
            config.pending_migrations.clear();
            let persist_recovery = (|| -> AppResult<()> {
                let tmp = path.with_extension("json.tmp");
                fs::write(&tmp, serde_json::to_string_pretty(&config)?)?;
                fs::rename(tmp, &path)?;
                Ok(())
            })();
            // Use recovered paths even when configuration is temporarily unwritable; do not
            // replace them with empty state.
            if let Err(error) = persist_recovery {
                eprintln!("Could not persist recovered relocation state: {error}");
            }
        }
        let mut normalized = false;
        for library in &mut config.libraries {
            let friendly = user_friendly_path(&library.path);
            if friendly != library.path {
                library.path = friendly;
                normalized = true;
            }
        }
        if normalized {
            if let Err(error) = config.save() {
                eprintln!("Could not persist the recovered library path: {error}");
            }
        }
        Ok(config)
    }

    /// Persist atomically through a temporary file and rename.
    pub fn save(&self) -> AppResult<()> {
        if let Some(error) = &self.load_error {
            return Err(crate::error::AppError::coded("config_load_failed", error));
        }
        let path = match &self.storage_path {
            Some(path) => path.clone(),
            None => Self::path()?,
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Store operation journals outside individual libraries because relocation moves the
    /// source root.
    pub fn operations_dir(&self) -> AppResult<PathBuf> {
        let path = match &self.storage_path {
            Some(path) => path.clone(),
            None => Self::path()?,
        };
        Ok(path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("operations"))
    }

    pub fn path() -> AppResult<PathBuf> {
        // Follow the platform application configuration directory convention.
        let dir = dirs_config_dir()?;
        Ok(dir.join("config.json"))
    }

    pub fn app_data_dir() -> AppResult<PathBuf> {
        Ok(Self::path()?
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf())
    }

    pub fn library_by_id(&self, id: &str) -> Option<&LibraryMeta> {
        self.libraries.iter().find(|l| l.id == id)
    }
}

/// Resolve configuration directories independently of the Tauri runtime for unit tests.
#[cfg(not(feature = "e2e"))]
fn dirs_config_dir() -> AppResult<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let p = PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("org.clipstudio.open");
            return Ok(p);
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(app_data) = std::env::var_os("APPDATA").filter(|value| !value.is_empty()) {
            return Ok(PathBuf::from(app_data).join("org.clipstudio.open"));
        }
    }
    Err(crate::error::AppError::Msg(
        "user config directory unavailable".into(),
    ))
}

#[cfg(test)]
#[path = "../tests/unit/global_config.rs"]
mod tests;

// Automation builds must fail before reading or writing a real user profile.
#[cfg(feature = "e2e")]
pub(crate) fn e2e_root() -> PathBuf {
    let root = PathBuf::from(
        std::env::var_os("CLIPSEEK_E2E_ROOT").expect("E2E builds require CLIPSEEK_E2E_ROOT"),
    );
    assert!(
        root.is_absolute() && root.is_dir(),
        "E2E root must be an existing absolute directory"
    );
    assert_eq!(
        root.canonicalize().expect("canonical E2E root"),
        root,
        "E2E root must not contain symlinks"
    );
    assert_eq!(
        fs::read_to_string(root.join(".clipseek-e2e-sandbox")).expect("E2E sandbox marker"),
        "clipseek-e2e-v1\n",
        "Invalid E2E sandbox marker"
    );
    root
}
#[cfg(feature = "e2e")]
fn dirs_config_dir() -> AppResult<PathBuf> {
    Ok(e2e_root().join("config"))
}

#[cfg(all(test, feature = "e2e"))]
#[path = "../tests/unit/e2e_isolation.rs"]
mod e2e_isolation;
