//! Link settings for `helio-host.exe`.
//!
//! Windows (MSVC): `/DEPENDENTLOADFLAG:0x800` (`LOAD_LIBRARY_SEARCH_SYSTEM32`)
//! has the loader look for the DLLs `helio-host.exe` imports in System32 only,
//! never beside it. `helio-host enable` runs elevated from the app's folder, which
//! its user can write to: a DLL put there under the name of one `helio-host.exe`
//! imports (`dwmapi.dll`, `wtsapi32.dll`, …) would otherwise load into the
//! elevated process before `main` runs. What is loaded after that,
//! `restrict_dll_search` in `main.rs` keeps to System32 and the program's
//! own folder.
//!
//! MinGW's linker has no such option, so a cross-build (development only)
//! goes without it.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os == "windows" && env == "msvc" {
        println!("cargo:rustc-link-arg-bins=/DEPENDENTLOADFLAG:0x800");
    }
}
