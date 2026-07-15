import { defineConfig } from "vitest/config";
import { execFileSync } from "node:child_process";
import react from "@vitejs/plugin-react";
import tailwindcss from '@tailwindcss/vite';

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

const buildId = (() => {
  // Release-candidate builds can keep the same semantic app version. The
  // source revision is the stable identity needed for consent migrations.
  const configured = process.env.VITE_BUILD_ID?.trim();
  if (configured) return configured;
  try {
    return execFileSync('git', ['rev-parse', 'HEAD'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore']
    }).trim() || 'unknown';
  } catch {
    return 'unknown';
  }
})();

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],
  define: {
    'import.meta.env.VITE_BUILD_ID': JSON.stringify(buildId)
  },
  test: {
    exclude: [
      "Extensions/**",
      "**/node_modules/**",
      "**/dist/**",
      "**/.{idea,git,cache,output,temp}/**"
    ]
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
