fn main() {
    if std::env::var("CARGO_FEATURE_GUI").is_ok() {
        println!("cargo:rerun-if-changed=icons/icon.png");
        println!("cargo:rerun-if-changed=icons/icon.ico");
        println!("cargo:rerun-if-changed=src-tauri/icons/icon.png");
        println!("cargo:rerun-if-changed=src-tauri/icons/icon.ico");
        tauri_build::build();
    }
}
