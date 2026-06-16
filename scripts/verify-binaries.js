const fs = require('fs');
const path = require('path');
const os = require('os');

// Maps Node's os.arch() to Rust's target_arch
const archMap = {
  'x64': 'x86_64',
  'arm64': 'aarch64'
};

// Maps Node's os.platform() to Rust's target_os/target_env
const platformMap = {
  'darwin': 'apple-darwin',
  'win32': 'pc-windows-msvc',
  'linux': 'unknown-linux-gnu'
};

const currentArch = archMap[os.arch()];
const currentPlatform = platformMap[os.platform()];

if (!currentArch || !currentPlatform) {
  console.error(`Unsupported architecture or platform: ${os.arch()} / ${os.platform()}`);
  process.exit(1);
}

const targetTriple = `${currentArch}-${currentPlatform}`;
const isWindows = os.platform() === 'win32';
const ext = isWindows ? '.exe' : '';
const suffix = `-${targetTriple}${ext}`;

const binariesDir = path.join(__dirname, '..', 'src-tauri', 'binaries');
const requiredBinaries = ['yt-dlp', 'aria2c', 'ffmpeg', 'deno'];

console.log(`Verifying target sidecars for: ${targetTriple}`);

let missing = false;

for (const bin of requiredBinaries) {
  const expectedName = `${bin}${suffix}`;
  const binPath = path.join(binariesDir, expectedName);

  if (!fs.existsSync(binPath)) {
    console.error(`[ERROR] Missing strictly required sidecar: ${expectedName} in src-tauri/binaries/`);
    missing = true;
  } else {
    console.log(`[OK] Found sidecar: ${expectedName}`);
  }
}

if (missing) {
  console.error('\nPlease download or build the missing target triple binaries and place them in the src-tauri/binaries directory.');
  console.error('Build blocked due to missing architecture-aware sidecars.');
  process.exit(1);
}

console.log('All required sidecars are present.');
process.exit(0);
