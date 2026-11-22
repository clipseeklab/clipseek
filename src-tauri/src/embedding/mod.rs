pub mod domain;
pub mod engine;
pub mod execution;
pub mod media;
pub mod models;
pub mod preprocess;

mod q8_metal;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod coreml;
