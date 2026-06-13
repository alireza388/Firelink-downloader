use std::net::TcpListener;
use std::io::{Read, Write};
use std::thread;
use tauri::{AppHandle, Emitter};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone)]
struct ExtensionPayload {
    url: String,
    token: String,
}

pub fn start_server(app_handle: AppHandle) {
    thread::spawn(move || {
        let listener = match TcpListener::bind("127.0.0.1:23522") {
            Ok(l) => l,
            Err(_) => return, // Port might be in use, ignore for now
        };

        for stream in listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    let mut buffer = [0; 1024];
                    if let Ok(size) = stream.read(&mut buffer) {
                        let request = String::from_utf8_lossy(&buffer[..size]);
                        
                        // Parse simple HTTP POST
                        if request.starts_with("POST") {
                            if let Some(body_start) = request.find("\r\n\r\n") {
                                let body = &request[body_start + 4..];
                                if let Ok(payload) = serde_json::from_str::<ExtensionPayload>(body) {
                                    // Emit event to frontend
                                    let _ = app_handle.emit("extension-add-download", payload);
                                    
                                    // Send success response
                                    let response = "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nContent-Type: application/json\r\n\r\n{\"success\":true}";
                                    let _ = stream.write_all(response.as_bytes());
                                    continue;
                                }
                            }
                        }
                        
                        // Handle OPTIONS for CORS
                        if request.starts_with("OPTIONS") {
                            let response = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\n\r\n";
                            let _ = stream.write_all(response.as_bytes());
                        } else {
                            let response = "HTTP/1.1 400 Bad Request\r\nAccess-Control-Allow-Origin: *\r\n\r\n";
                            let _ = stream.write_all(response.as_bytes());
                        }
                    }
                }
                Err(_) => {}
            }
        }
    });
}
