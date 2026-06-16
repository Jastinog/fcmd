use std::env;

fn main() {
    // Custom cfg `trash_os_limited`: platforms where the `trash` crate exposes
    // its `os_limited` API for enumerating and restoring trashed items
    // (Freedesktop unix excluding macOS/iOS/Android, and Windows). macOS only
    // supports `delete`, so it is handled separately. Declared here so it can
    // be set conditionally without tripping `unexpected_cfgs` lints.
    println!("cargo:rustc-check-cfg=cfg(trash_os_limited)");

    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let family = env::var("CARGO_CFG_TARGET_FAMILY").unwrap_or_default();
    let is_unix = family.split(',').any(|f| f == "unix");
    let freedesktop = is_unix && !matches!(os.as_str(), "macos" | "ios" | "android");
    if os == "windows" || freedesktop {
        println!("cargo:rustc-cfg=trash_os_limited");
    }
}
