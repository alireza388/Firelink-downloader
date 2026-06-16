import re
with open("src-tauri/src/lib.rs", "r") as f:
    content = f.read()

content = content.replace('let text = String::from_utf8_lossy(&output.stdout).to_string();\n        Ok(text)', 'let text = String::from_utf8_lossy(&output.stdout).to_string();\n        std::fs::write("/tmp/firelink_out.txt", &text).unwrap();\n        Ok(text)')
content = content.replace('let err = String::from_utf8_lossy(&output.stderr);\n        Err(format!("yt-dlp error: {}", err))', 'let err = String::from_utf8_lossy(&output.stderr);\n        std::fs::write("/tmp/firelink_err.txt", &err).unwrap();\n        Err(format!("yt-dlp error: {}", err))')

with open("src-tauri/src/lib.rs", "w") as f:
    f.write(content)
