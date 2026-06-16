with open("src-tauri/src/main.rs", "r") as f:
    content = f.read()

test_call = """
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                println!("Testing fetch_media_metadata...");
                let res = firelink::fetch_media_metadata(
                    handle, 
                    "https://www.youtube.com/watch?v=a769AIuHOdE".to_string(), 
                    None, None, None
                ).await;
                match res {
                    Ok(out) => println!("TEST SUCCESS: {}", out.chars().take(200).collect::<String>()),
                    Err(e) => println!("TEST ERROR: {}", e)
                }
            });
"""

if "Testing fetch_media_metadata" not in content:
    content = content.replace('.setup(|app| {', test_call)
    with open("src-tauri/src/main.rs", "w") as f:
        f.write(content)
print("Added test call to main.rs")
