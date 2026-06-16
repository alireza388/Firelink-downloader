const fs = require('fs');
const appTsxPath = 'src/App.tsx';
let appTsx = fs.readFileSync(appTsxPath, 'utf8');

const injection = `
import { invoke } from '@tauri-apps/api/core';
useEffect(() => {
  invoke('fetch_media_metadata', {
    url: "https://www.youtube.com/watch?v=a769AIuHOdE",
    cookieBrowser: null,
    username: null,
    password: null
  }).then(res => {
    invoke('perform_system_action', { action: "TEST_SUCCESS: " + res.substring(0, 50) });
  }).catch(err => {
    invoke('perform_system_action', { action: "TEST_ERROR: " + String(err) });
  });
}, []);
`;

// Insert after imports
if (!appTsx.includes('TEST_ERROR')) {
    appTsx = appTsx.replace('export default function App() {', "export default function App() {\n" + injection);
    fs.writeFileSync(appTsxPath, appTsx);
}
