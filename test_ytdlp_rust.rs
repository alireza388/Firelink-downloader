use std::process::Command;

fn main() {
    let ytdlp_path = "/Users/nima/Documents/Code/Firelink/src-tauri/binaries/yt-dlp";
    let deno_path = "/Users/nima/Documents/Code/Firelink/src-tauri/binaries/deno";
    
    let mut cmd = Command::new(ytdlp_path);
    cmd.arg("-J")
       .arg("--no-warnings")
       .arg("--no-playlist")
       .arg("--no-check-formats")
       .arg("--socket-timeout").arg("20")
       .arg("--retries").arg("3")
       .arg("--extractor-retries").arg("3")
       .arg("--compat-options").arg("no-youtube-unavailable-videos")
       .arg("--js-runtimes").arg(format!("deno:{},node", deno_path))
       .arg("--")
       .arg("https://www.youtube.com/watch?v=a769AIuHOdE");

    println!("Running: {:?}", cmd);
    let output = cmd.output().unwrap();
    println!("Status: {}", output.status);
    println!("Stdout: {}", String::from_utf8_lossy(&output.stdout).chars().take(200).collect::<String>());
    println!("Stderr: {}", String::from_utf8_lossy(&output.stderr));
}
