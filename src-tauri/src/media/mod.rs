pub mod ffmpeg;
pub mod metadata;
mod photo_exif;
pub mod raw_video;
pub mod sips;

use std::path::PathBuf;
use std::process::Command;

/// Resolve FFmpeg using CLIPSTUDIO_FFMPEG, bundled Windows resources, the executable
/// directory, or system PATH.
pub fn ffmpeg_path() -> PathBuf {
    if let Ok(p) = std::env::var("CLIPSTUDIO_FFMPEG") {
        return PathBuf::from(p);
    }
    let runtime = crate::model_download::runtime().join(if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    });
    if runtime.is_file() {
        return runtime;
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            #[cfg(target_os = "windows")]
            for ancestor in dir.ancestors().take(8) {
                for candidate in [
                    ancestor.join("ffmpeg.exe"),
                    ancestor.join("binaries/ffmpeg.exe"),
                ] {
                    if candidate.is_file() {
                        return candidate;
                    }
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                let candidate = dir.join("ffmpeg");
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }
    PathBuf::from(if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    })
}

/// Hide the FFmpeg console window when launching from the Windows GUI.
pub fn ffmpeg_command() -> Command {
    let command = Command::new(ffmpeg_path());
    #[cfg(target_os = "windows")]
    let command = {
        let mut command = command;
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        command
    };
    command
}

/// Check FFmpeg availability once during startup.
pub fn ffmpeg_available() -> bool {
    ffmpeg_command()
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
