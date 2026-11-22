//! Synchronous, owned CoreML predictions behind the shared encoder mutex.
use anyhow::{bail, Context, Result};
use libloading::Library;
use std::{
    ffi::{c_char, c_void, CStr, CString},
    fs,
    io::Write,
    path::Path,
    sync::{Arc, Mutex},
};
type Compile = unsafe extern "C" fn(*const c_char, *const c_char, *mut c_char, usize) -> i32;
type Open = unsafe extern "C" fn(*const c_char, *const c_char, *mut c_char, usize) -> *mut c_void;
type Close = unsafe extern "C" fn(*mut c_void);
type Vision =
    unsafe extern "C" fn(*mut c_void, *const f32, *const i32, *mut f32, *mut c_char, usize) -> i32;
type Body = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    *const f32,
    *const i32,
    *const i32,
    *const f32,
    usize,
    *mut f32,
    *mut c_char,
    usize,
) -> i32;
pub(super) struct Bridge {
    _library: Library,
    compile: Compile,
    open: Open,
    close: Close,
    vision: Vision,
    body: Body,
}
pub(super) struct Handle {
    bridge: Arc<Bridge>,
    pointer: *mut c_void,
}
// Swift copies every borrowed input. Engine's mutex serializes model and LRU access.
unsafe impl Send for Handle {}
static SHARED: Mutex<Option<Arc<Bridge>>> = Mutex::new(None);
fn path(path: &Path) -> Result<CString> {
    Ok(CString::new(
        path.to_str().context("Invalid native cache path")?,
    )?)
}
fn message(buffer: &[c_char]) -> String {
    unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}
pub(super) fn supported_version(version: &str) -> bool {
    version
        .trim()
        .split('.')
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .is_some_and(|major| major >= 15)
}
impl Bridge {
    pub fn shared(root: &Path) -> Result<Arc<Self>> {
        let mut shared = SHARED
            .lock()
            .map_err(|_| anyhow::anyhow!("Native bridge lock poisoned"))?;
        if let Some(bridge) = shared.as_ref() {
            return Ok(Arc::clone(bridge));
        }
        let version = std::process::Command::new("/usr/bin/sw_vers")
            .arg("-productVersion")
            .output()?;
        if !version.status.success() || !supported_version(&String::from_utf8(version.stdout)?) {
            bail!("Native CoreML GPU requires macOS 15 or later");
        }
        let bytes = include_bytes!(env!("CLIPSTUDIO_NATIVE_COREML"));
        let directory = root
            .parent()
            .context("Missing model parent")?
            .join(".native-coreml");
        fs::create_dir_all(&directory)?;
        let destination = directory.join(format!(
            "bridge-{}.dylib",
            &blake3::hash(bytes).to_hex()[..16]
        ));
        if !destination.exists() || fs::read(&destination)? != bytes {
            let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
            temporary.write_all(bytes)?;
            temporary.as_file().sync_all()?;
            temporary.persist(&destination)?;
        }
        let bridge = unsafe {
            let library = Library::new(&destination)?;
            Self {
                compile: *library.get(b"cs_coreml_compile")?,
                open: *library.get(b"cs_coreml_open")?,
                close: *library.get(b"cs_coreml_close")?,
                vision: *library.get(b"cs_coreml_vision")?,
                body: *library.get(b"cs_coreml_body")?,
                _library: library,
            }
        };
        // Keep one library loaded for the process lifetime; Swift classes cannot be safely reloaded.
        let bridge = Arc::new(bridge);
        *shared = Some(Arc::clone(&bridge));
        Ok(bridge)
    }
    pub fn compile(&self, source: &Path, destination: &Path) -> Result<()> {
        let mut error = [0; 2048];
        if unsafe {
            (self.compile)(
                path(source)?.as_ptr(),
                path(destination)?.as_ptr(),
                error.as_mut_ptr(),
                error.len(),
            )
        } != 0
        {
            bail!("CoreML compilation failed: {}", message(&error));
        }
        Ok(())
    }
    pub fn open(self: &Arc<Self>, directory: &Path) -> Result<Handle> {
        let mut error = [0; 2048];
        let pointer = unsafe {
            (self.open)(
                path(&directory.join("vision.mlmodelc"))?.as_ptr(),
                path(&directory.join("body-32.mlmodelc"))?.as_ptr(),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if pointer.is_null() {
            bail!("CoreML initialization failed: {}", message(&error));
        }
        Ok(Handle {
            bridge: Arc::clone(self),
            pointer,
        })
    }
}
impl Handle {
    pub fn vision(&mut self, pixels: &[f32], positions: &[i32]) -> Result<Vec<f32>> {
        let mut output = vec![0.; 280 * 512];
        let mut error = [0; 2048];
        if pixels.len() != 2520 * 768 || positions.len() != 2520 * 2 {
            bail!("Invalid native vision buffer");
        }
        if unsafe {
            (self.bridge.vision)(
                self.pointer,
                pixels.as_ptr(),
                positions.as_ptr(),
                output.as_mut_ptr(),
                error.as_mut_ptr(),
                error.len(),
            )
        } != 0
        {
            bail!("CoreML vision failed: {}", message(&error));
        }
        Ok(output)
    }
    pub fn body(
        &mut self,
        model: &Path,
        embedded: &[f32],
        ids: &[i32],
        mask: &[i32],
        positions: &[f32],
    ) -> Result<Vec<f32>> {
        let n = ids.len();
        if ![32, 128, 256, 512, 1024, 2048, 2560, 4096, 4608, 8192].contains(&n)
            || embedded.len() != n * 512
            || mask.len() != n
            || positions.len() != n
        {
            bail!("Invalid native backbone buffer");
        }
        let mut output = vec![0.; 768];
        let mut error = [0; 2048];
        if unsafe {
            (self.bridge.body)(
                self.pointer,
                path(model)?.as_ptr(),
                embedded.as_ptr(),
                ids.as_ptr(),
                mask.as_ptr(),
                positions.as_ptr(),
                n,
                output.as_mut_ptr(),
                error.as_mut_ptr(),
                error.len(),
            )
        } != 0
        {
            bail!("CoreML prediction failed: {}", message(&error));
        }
        Ok(output)
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { (self.bridge.close)(self.pointer) };
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/coreml_bridge.rs"]
mod tests;
