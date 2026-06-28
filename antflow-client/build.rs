fn download_model_files() {
    let out_dir = std::path::Path::new("src").join("model");
    std::fs::create_dir_all(&out_dir).unwrap();

    let files = [
        ("config.json", "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/config.json"),
        ("tokenizer.json", "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json"),
        ("model.safetensors", "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/model.safetensors"),
    ];

    for (name, url) in files {
        let dest = out_dir.join(name);
        if !dest.exists() {
            println!("cargo:warning=Downloading all-MiniLM-L6-v2 {} from Hugging Face...", name);
            let response = reqwest::blocking::get(url);
            match response {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let mut file = std::fs::File::create(&dest).expect("Failed to create file");
                        let mut bytes = resp.bytes().expect("Failed to read bytes");
                        std::io::copy(&mut bytes.as_ref(), &mut file).expect("Failed to copy bytes");
                        println!("cargo:warning=Successfully downloaded {}!", name);
                    } else {
                        panic!("Failed to download {}: HTTP status {}", name, resp.status());
                    }
                }
                Err(e) => {
                    panic!("Failed to download {}: {:?}", name, e);
                }
            }
        }
    }
}

fn copy_dir_all(src: impl AsRef<std::path::Path>, dst: impl AsRef<std::path::Path>) -> std::io::Result<()> {
    std::fs::create_dir_all(&dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn main() {
    // Ensure all-MiniLM-L6-v2 model files are downloaded and cached
    download_model_files();

    if std::env::var("CARGO_FEATURE_GUI").is_ok() {
        // Ensure dist/ exists and is populated before running tauri_build::build()
        let src_dir = std::path::Path::new("src-tauri").join("frontend");
        let dst_dir = std::path::Path::new("dist");
        if !dst_dir.exists() || std::fs::read_dir(&dst_dir).map(|mut r| r.next().is_none()).unwrap_or(true) {
            let _ = copy_dir_all(&src_dir, &dst_dir);
        }

        println!("cargo:rerun-if-changed=icons/icon.png");
        println!("cargo:rerun-if-changed=icons/icon.ico");
        println!("cargo:rerun-if-changed=src-tauri/icons/icon.png");
        println!("cargo:rerun-if-changed=src-tauri/icons/icon.ico");
        tauri_build::build();
    }
}
