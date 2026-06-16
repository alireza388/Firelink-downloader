with open("src-tauri/src/lib.rs", "r") as f:
    content = f.read()

content = content.replace('println!("fetch_media_metadata called for: {}", url);', 'println!("fetch_media_metadata called for: {}", url);')
content = content.replace('let err = String::from_utf8_lossy(&output.stderr);', 'let err = String::from_utf8_lossy(&output.stderr); println!("YTDLP ERROR: {}", err);')
content = content.replace('let text = String::from_utf8_lossy(&output.stdout).to_string();', 'let text = String::from_utf8_lossy(&output.stdout).to_string(); println!("YTDLP SUCCESS: {}", text.chars().take(200).collect::<String>());')

with open("src-tauri/src/lib.rs", "w") as f:
    f.write(content)
