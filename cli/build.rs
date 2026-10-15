// Windows only, and only for debug builds (which is what `cargo test` makes).
//
// The tests link the desktop app's code. The app's build script (tauri-build) links the Windows C runtime
// statically: it puts an empty `msvcrt.lib` first on the library path and adds link arguments that say which
// runtime libraries to use instead. The library path is inherited by every crate that depends on the app, but
// the link arguments are given only to the app's own programs. Without the arguments a test here would find the
// empty `msvcrt.lib` and no C runtime at all, and the link fails with hundreds of "unresolved external symbol".
// These are the same arguments, so the tests link the way the app's own tests do.
//
// A release build (`cargo install --path cli`) does not link the app and is left as it is.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let var = |name: &str| std::env::var(name).unwrap_or_default();
    if var("CARGO_CFG_TARGET_OS") != "windows" || var("CARGO_CFG_TARGET_ENV") != "msvc" || var("PROFILE") != "debug" {
        return;
    }
    for arg in [
        "/NODEFAULTLIB:libvcruntimed.lib",
        "/NODEFAULTLIB:vcruntime.lib",
        "/NODEFAULTLIB:vcruntimed.lib",
        "/NODEFAULTLIB:libcmtd.lib",
        "/NODEFAULTLIB:msvcrt.lib",
        "/NODEFAULTLIB:msvcrtd.lib",
        "/NODEFAULTLIB:libucrt.lib",
        "/NODEFAULTLIB:libucrtd.lib",
        "/DEFAULTLIB:libcmt.lib",
        "/DEFAULTLIB:libvcruntime.lib",
        "/DEFAULTLIB:ucrt.lib",
    ] {
        println!("cargo:rustc-link-arg={arg}");
    }
}
