import { fileURLToPath } from 'node:url'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vite'

const uiRoot = fileURLToPath(new URL('./ui', import.meta.url))
const iconsDir = fileURLToPath(new URL('../assets/icons', import.meta.url))

// Settings window for the Tauri app. Source lives in `ui/`, output goes to
// `dist/`, which `src-tauri/tauri.conf.json` bundles into the app.
export default defineConfig({
  root: uiRoot,
  // svelte.config.js sits next to this file (where svelte-check also looks), not in `ui/`.
  plugins: [svelte({ configFile: fileURLToPath(new URL('./svelte.config.js', import.meta.url)) })],
  // Keep Rust compiler errors visible when running `tauri dev`.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // Allow importing the shared app icon from the repo's assets folder.
    fs: { allow: [uiRoot, iconsDir] },
  },
  build: {
    outDir: '../dist',
    emptyOutDir: true,
  },
})
