fn main() {
    // Lets `otlk update` pick the release archive built for this target.
    println!(
        "cargo:rustc-env=TUILOOK_TARGET={}",
        std::env::var("TARGET").expect("Cargo sets TARGET for build scripts")
    );
    // Embeds the application icon in otlk.exe. Other platforms have no
    // equivalent for a command-line binary.
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/icon.ico")
            .compile()
            .expect("embed the Windows icon");
    }
}
