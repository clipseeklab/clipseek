//! Detect network volumes such as NAS/SMB/NFS. SQLite WAL, file locks, and mmap require local
//! filesystem semantics, so new libraries and relocations reject network volumes; existing
//! libraries show migration advice.

use std::path::{Path, PathBuf};

/// Nonblocking network-volume status for existing libraries; use ensure_local_volume for
/// strict validation that rejects failed detection.
pub fn is_network_volume(path: &Path) -> bool {
    detect(path).map(|(network, _)| network).unwrap_or(false)
}

/// Filesystem type, such as apfs or smbfs, for logs and diagnostics.
pub fn volume_type_name(path: &Path) -> Option<String> {
    detect(path).map(|(_, name)| name)
}

/// Require a local library volume because SQLite WAL and vector locks are unreliable on
/// network storage.
pub fn ensure_local_volume(path: &Path) -> crate::error::AppResult<()> {
    match detect(path) {
        Some((false, _)) => Ok(()),
        Some((true, kind)) => Err(crate::error::AppError::coded(
            "library_network_volume",
            format!(
                "library path is on a network volume ({kind}): {}",
                path.display()
            ),
        )),
        None => Err(crate::error::AppError::coded(
            "library_volume_unknown",
            format!(
                "unable to verify that the library path is on a local volume: {}",
                path.display()
            ),
        )),
    }
}

/// Walk to an existing ancestor before statfs when the requested library path does not yet
/// exist.
fn stat_target(path: &Path) -> Option<PathBuf> {
    let mut current = path.to_path_buf();
    loop {
        if current.exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn detect(path: &Path) -> Option<(bool, String)> {
    let target = stat_target(path)?;
    let fs = nix::sys::statfs::statfs(&target).ok()?;

    #[cfg(target_os = "macos")]
    {
        // Treat a missing MNT_LOCAL flag as network storage, with filesystem-name fallbacks
        // for incomplete implementations.
        let name = fs.filesystem_type_name().to_lowercase();
        let network = !fs.flags().contains(nix::mount::MntFlags::MNT_LOCAL)
            || ["nfs", "smbfs", "afpfs", "cifs", "webdav", "kafpfs"].contains(&name.as_str());
        Some((network, name))
    }

    #[cfg(target_os = "linux")]
    {
        // Common network-filesystem statfs magic values from linux/magic.h.
        const NFS_SUPER_MAGIC: i64 = 0x6969;
        const SMB_SUPER_MAGIC: i64 = 0x517b;
        const CIFS_MAGIC_NUMBER: i64 = 0xff53_4d42;
        const SMB2_MAGIC_NUMBER: i64 = 0xfe53_4d42;
        const FUSE_SUPER_MAGIC: i64 = 0x6573_5546;
        const NINE_P_SUPER_MAGIC: i64 = 0x0102_1997;
        const CEPH_SUPER_MAGIC: i64 = 0x00c3_64_00;
        const AFS_SUPER_MAGIC: i64 = 0x5346_414f;
        let (network, name) = match fs.filesystem_type().0 as i64 {
            NFS_SUPER_MAGIC => (true, "nfs"),
            SMB_SUPER_MAGIC => (true, "smb"),
            CIFS_MAGIC_NUMBER => (true, "cifs"),
            SMB2_MAGIC_NUMBER => (true, "smb2"),
            // FUSE cannot reliably distinguish local implementations from sshfs/rclone;
            // reject it because WAL and mmap require known local semantics.
            FUSE_SUPER_MAGIC => (true, "fuse"),
            NINE_P_SUPER_MAGIC => (true, "9p"),
            CEPH_SUPER_MAGIC => (true, "ceph"),
            AFS_SUPER_MAGIC => (true, "afs"),
            _ => (false, "local"),
        };
        Some((network, name.to_string()))
    }
}

#[cfg(target_os = "windows")]
fn detect(path: &Path) -> Option<(bool, String)> {
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Component, Prefix};
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;

    const DRIVE_REMOTE: u32 = 4;
    const DRIVE_UNKNOWN: u32 = 0;
    const DRIVE_NO_ROOT_DIR: u32 = 1;

    // canonicalize follows junctions and produces \\?\C:\... for ordinary local
    // paths. A leading "\\" alone therefore cannot distinguish UNC from a disk.
    let target = std::fs::canonicalize(stat_target(path)?).ok()?;
    let Component::Prefix(prefix) = target.components().next()? else {
        return None;
    };
    let (root, name) = match prefix.kind() {
        Prefix::UNC(..) | Prefix::VerbatimUNC(..) => return Some((true, "unc".into())),
        Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
            let letter = (letter as char).to_ascii_uppercase();
            (
                PathBuf::from(format!("{letter}:\\")),
                format!("{}:", letter.to_ascii_lowercase()),
            )
        }
        Prefix::Verbatim(_) => (
            PathBuf::from(format!("{}\\", prefix.as_os_str().to_string_lossy())),
            "volume".into(),
        ),
        Prefix::DeviceNS(_) => return None,
    };
    let mut wide: Vec<u16> = root.as_os_str().encode_wide().collect();
    wide.push(0);
    let kind = unsafe { GetDriveTypeW(wide.as_ptr()) };
    if matches!(kind, DRIVE_UNKNOWN | DRIVE_NO_ROOT_DIR) {
        return None;
    }
    Some((kind == DRIVE_REMOTE, name))
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn detect(_path: &Path) -> Option<(bool, String)> {
    None
}
