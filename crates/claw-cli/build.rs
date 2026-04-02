//! Windows: raise PE stack reserve for the `claw` binary (belt-and-suspenders with the GUI thread).

fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("windows-msvc") {
        // Prefer `rustc-link-arg-bins` so the flag applies to `[[bin]]` targets.
        // 128 MiB — main-thread GUI + full CLI needs a large reserve on Windows PE defaults.
        println!("cargo:rustc-link-arg-bins=/STACK:134217728");
    } else if target.contains("windows-gnu") {
        println!("cargo:rustc-link-arg-bins=-Wl,--stack,134217728");
    }
}
