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

fn main() {
    // Ensure all-MiniLM-L6-v2 model files are downloaded and cached
    download_model_files();

    if std::env::var("CARGO_FEATURE_GUI").is_ok() {
        println!("cargo:rerun-if-changed=icons/icon.png");
        println!("cargo:rerun-if-changed=icons/icon.ico");
        println!("cargo:rerun-if-changed=src-tauri/icons/icon.png");
        println!("cargo:rerun-if-changed=src-tauri/icons/icon.ico");
        tauri_build::build();
    }
}
