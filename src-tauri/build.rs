fn build_native_video() {
    println!("cargo:rerun-if-changed=native/video.swift");
    println!("cargo:rerun-if-env-changed=APPLE_SIGNING_IDENTITY");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        other => panic!("Unsupported macOS architecture: {other:?}"),
    };
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
        .join("clipseek-native-video");
    let mut compiler = std::process::Command::new("xcrun");
    compiler.arg("swiftc");
    if arch == "x86_64" {
        println!("cargo:rerun-if-env-changed=CLIPSEEK_INTEL_SWIFT_RESOURCE_DIR");
        if let Some(resources) =
            std::env::var_os("CLIPSEEK_INTEL_SWIFT_RESOURCE_DIR").filter(|path| !path.is_empty())
        {
            let resources = std::path::PathBuf::from(resources);
            for archive in ["libswiftCompatibility56.a", "libswiftCompatibilityPacks.a"] {
                println!(
                    "cargo:rerun-if-changed={}",
                    resources.join("macosx").join(archive).display()
                );
            }
            compiler.arg("-resource-dir").arg(resources);
        }
    }
    let status = compiler
        .args([
            "-O",
            "-target",
            &format!("{arch}-apple-macos12.0"),
            "-module-cache-path",
        ])
        .arg(output.parent().unwrap().join("swift-module-cache"))
        .arg("native/video.swift")
        .arg("-o")
        .arg(&output)
        .status()
        .expect("macOS builds require the Xcode Swift command line tools");
    assert!(status.success(), "Failed to build the native video helper");
    // Release builds embed a Developer ID signed helper. Local builds use the
    // compiler's ad-hoc signature; runtime extraction preserves these bytes.
    if let Ok(identity) = std::env::var("APPLE_SIGNING_IDENTITY") {
        if !identity.is_empty() {
            let status = std::process::Command::new("codesign")
                .args([
                    "--force",
                    "--timestamp",
                    "--options",
                    "runtime",
                    "--sign",
                    &identity,
                ])
                .arg(&output)
                .status()
                .expect("Unable to sign the native video helper");
            assert!(status.success(), "Native video helper code signing failed");
        }
    }
    println!("cargo:rustc-env=CLIPSEEK_NATIVE_VIDEO={}", output.display());
}

fn build_native_coreml() {
    println!("cargo:rerun-if-changed=native/coreml.swift");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos")
        || std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("aarch64")
    {
        return;
    }
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
        .join("libclipstudio-coreml.dylib");
    let status = std::process::Command::new("xcrun")
        .args([
            "swiftc",
            "-O",
            "-emit-library",
            "-module-name",
            "ClipStudioNativeCoreML",
            "-target",
            "arm64-apple-macos15.0",
            "-module-cache-path",
        ])
        .arg(output.parent().unwrap().join("swift-module-cache"))
        .arg("native/coreml.swift")
        .arg("-o")
        .arg(&output)
        .status()
        .expect("CoreML requires the Xcode Swift tools");
    assert!(status.success(), "Failed to build the native CoreML bridge");
    if let Ok(identity) = std::env::var("APPLE_SIGNING_IDENTITY") {
        if !identity.is_empty() {
            let status = std::process::Command::new("codesign")
                .args([
                    "--force",
                    "--timestamp",
                    "--options",
                    "runtime",
                    "--sign",
                    &identity,
                ])
                .arg(&output)
                .status()
                .expect("Unable to sign CoreML bridge");
            assert!(status.success(), "CoreML bridge signing failed");
        }
    }
    println!(
        "cargo:rustc-env=CLIPSTUDIO_NATIVE_COREML={}",
        output.display()
    );
}

fn main() {
    println!("cargo:rerun-if-env-changed=VITE_E2E");
    assert!(
        std::env::var("VITE_E2E").as_deref() != Ok("true")
            || std::env::var_os("CARGO_FEATURE_E2E").is_some(),
        "E2E frontend assets require the dedicated e2e Cargo feature"
    );
    build_native_coreml();
    build_native_video();
    let mut attributes = tauri_build::Attributes::new();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        // Tauri embeds resources only in app binaries. Dialog-linked Rust tests also
        // need Common Controls v6, so let the linker embed the manifest for all targets.
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    }
    tauri_build::try_build(attributes).expect("Failed to build Tauri resources");
}
