//! Check app-local x64 VC++ DLLs before any ONNX Runtime initialization.

use std::ffi::c_void;
use std::fmt;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

pub const MINIMUM_VERSION: Version = Version::new(14, 44, 35211, 0);
pub const RUNTIME_DLLS: [&str; 4] = [
    "MSVCP140.dll",
    "MSVCP140_1.dll",
    "VCRUNTIME140.dll",
    "VCRUNTIME140_1.dll",
];

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Version {
    major: u16,
    minor: u16,
    build: u16,
    revision: u16,
}

impl Version {
    pub const fn new(major: u16, minor: u16, build: u16, revision: u16) -> Self {
        Self {
            major,
            minor,
            build,
            revision,
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{}.{}.{}",
            self.major, self.minor, self.build, self.revision
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeStatus {
    Ready,
    Missing,
    TooOld(Version),
}

pub fn assess_version(version: Option<Version>) -> RuntimeStatus {
    match version {
        Some(version) if version >= MINIMUM_VERSION => RuntimeStatus::Ready,
        Some(version) => RuntimeStatus::TooOld(version),
        None => RuntimeStatus::Missing,
    }
}

pub fn check() -> RuntimeStatus {
    let Some(directory) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
    else {
        return RuntimeStatus::Missing;
    };
    for dll in RUNTIME_DLLS {
        let status = assess_version(file_version(&directory.join(dll)));
        if status != RuntimeStatus::Ready {
            return status;
        }
    }
    RuntimeStatus::Ready
}

pub fn show_error(status: RuntimeStatus) {
    let detail = match status {
        RuntimeStatus::Ready => return,
        RuntimeStatus::Missing => {
            "The bundled VC++ runtime DLL is missing or unreadable.".to_owned()
        }
        RuntimeStatus::TooOld(version) => {
            format!("The bundled VC++ runtime DLL is outdated: {version}")
        }
    };
    let message = format!(
        "ClipSeek Open requires the bundled Microsoft Visual C++ x64 runtime {MINIMUM_VERSION} or later.\n{detail}\n\nPlease reinstall ClipSeek Open."
    );
    let title = wide("ClipSeek Open - Required runtime");
    let message = wide(&message);
    unsafe {
        MessageBoxW(std::ptr::null_mut(), message.as_ptr(), title.as_ptr(), 0x30);
    }
}

fn file_version(path: &Path) -> Option<Version> {
    let filename = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut handle = 0;
    let size = unsafe { GetFileVersionInfoSizeW(filename.as_ptr(), &mut handle) };
    if size == 0 {
        return None;
    }
    // A u32 buffer keeps VS_FIXEDFILEINFO aligned even if the API writes arbitrary bytes.
    let mut data = vec![0u32; (size as usize).div_ceil(4)];
    if unsafe { GetFileVersionInfoW(filename.as_ptr(), 0, size, data.as_mut_ptr().cast()) } == 0 {
        return None;
    }
    let mut value = std::ptr::null_mut();
    let mut length = 0;
    let root = wide("\\");
    if unsafe { VerQueryValueW(data.as_ptr().cast(), root.as_ptr(), &mut value, &mut length) } == 0
        || length < 16
        || value.is_null()
    {
        return None;
    }
    // VS_FIXEDFILEINFO starts with signature, structure version, file-version high/low DWORDs.
    let words = value.cast::<u32>();
    let signature = unsafe { words.read_unaligned() };
    if signature != 0xFEEF04BD {
        return None;
    }
    let high = unsafe { words.add(2).read_unaligned() };
    let low = unsafe { words.add(3).read_unaligned() };
    Some(Version::new(
        (high >> 16) as u16,
        high as u16,
        (low >> 16) as u16,
        low as u16,
    ))
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[link(name = "version")]
extern "system" {
    fn GetFileVersionInfoSizeW(filename: *const u16, handle: *mut u32) -> u32;
    fn GetFileVersionInfoW(
        filename: *const u16,
        handle: u32,
        length: u32,
        data: *mut c_void,
    ) -> i32;
    fn VerQueryValueW(
        data: *const c_void,
        sub_block: *const u16,
        buffer: *mut *mut c_void,
        length: *mut u32,
    ) -> i32;
}

#[link(name = "user32")]
extern "system" {
    fn MessageBoxW(window: *mut c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
}
