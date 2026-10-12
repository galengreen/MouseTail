fn main() {
    // Sending the Mac's sound uses Core Audio's process taps, which arrived in macOS 14.2.
    // Weak-linking Core Audio lets the daemon still load on Ventura and 14.0/14.1, where
    // `SystemSound::supported` keeps it from calling them.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg-bins=-Wl,-weak_framework,CoreAudio");
    }
}
