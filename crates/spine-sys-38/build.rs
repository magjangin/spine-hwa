use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();

    let vendor_dir = workspace_root.join("vendor").join("spine-runtimes").join("3.8");
    let include_dir = vendor_dir.join("include");
    let src_dir = vendor_dir.join("src").join("spine");
    let c_src_dir = workspace_root.join("c_src");

    let mut build = cc::Build::new();
    build.cpp(true);
    build.std("c++17");
    build.include(&include_dir);
    build.include(&c_src_dir);

    if let Ok(entries) = std::fs::read_dir(&src_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("cpp") {
                let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if filename.contains("sfml") {
                    continue;
                }
                build.file(&path);
            }
        }
    }

    build.file(c_src_dir.join("spine_bridge.cpp"));
    build.define("_CRT_SECURE_NO_WARNINGS", None);
    if cfg!(target_env = "msvc") {
        build.flag("/EHsc");
    }

    build.compile("spine_bridge");

    println!("cargo:rerun-if-changed={}", c_src_dir.join("spine_bridge.h").display());
    println!("cargo:rerun-if-changed={}", c_src_dir.join("spine_bridge.cpp").display());
}
