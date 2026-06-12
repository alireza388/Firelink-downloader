import { invoke } from '@tauri-apps/api/core';
invoke('start_media_download', { id: "test", url: "https://www.youtube.com/watch?v=1PBRhm5ZnjU", destination: "/tmp", filename: "test.mp4", formatSelector: null }).then(console.log).catch(console.error);
