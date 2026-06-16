with open("src-tauri/src/lib.rs", "r") as f:
    content = f.read()

setup_code = """
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                println!("TESTING FETCH MEDIA METADATA NOW!");
                let res = fetch_media_metadata(
                    handle, 
                    "https://www.youtube.com/watch?v=a769AIuHOdE".to_string(), 
                    None, None, None
                ).await;
                match res {
                    Ok(out) => println!("TEST_SUCCESS: {}", out.chars().take(200).collect::<String>()),
                    Err(e) => println!("TEST_ERROR: {}", e)
                }
            });
            Ok(())
        })
"""

if ".setup(|app|" not in content:
    content = content.replace('.plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {', setup_code + '        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {')
    with open("src-tauri/src/lib.rs", "w") as f:
        f.write(content)
    print("Injected setup code.")
else:
    print("Setup code already exists?")
