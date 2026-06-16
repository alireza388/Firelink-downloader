const fs = require('fs');
let content = fs.readFileSync('src/App.tsx', 'utf8');
const idx1 = content.indexOf('import { invoke }');
const idx2 = content.indexOf('export default function App() {', idx1);
if (idx1 !== -1 && idx2 !== -1) {
    // wait I injected it into App function body, wait where did I inject it?
}
// I will just git checkout src/App.tsx
