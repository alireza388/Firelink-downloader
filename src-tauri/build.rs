fn main() {
    std::fs::create_dir_all("engine-dist")
        .expect("failed to create generated engine resource directory");
    tauri_build::build()
}
