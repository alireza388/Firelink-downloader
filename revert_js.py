with open("src-tauri/src/lib.rs", "r") as f:
    content = f.read()

content = content.replace('.arg("--js-runtimes").arg(format!("deno:{},node", deno_path.display()));', '.arg("--js-runtimes").arg("deno,node");')

with open("src-tauri/src/lib.rs", "w") as f:
    f.write(content)
