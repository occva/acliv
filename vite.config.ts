import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import packageJson from './package.json'

// https://vitejs.dev/config/
export default defineConfig({
    plugins: [svelte()],
    base: './', // Ensure relative paths for Tauri
    define: {
        'import.meta.env.VITE_APP_VERSION': JSON.stringify(packageJson.version),
    },


    // Vite options tailored for Tauri development
    clearScreen: false,

    server: {
        port: 1420,
        strictPort: true,
        watch: {
            ignored: ["**/src-tauri/**"],
        },
    },

    build: {
        // Tauri uses Chromium on Windows and WebKit on macOS and Linux
        target: process.env.TAURI_PLATFORM == 'windows' ? 'chrome105' : 'safari13',
        // don't minify for debug builds
        minify: !process.env.TAURI_DEBUG ? 'esbuild' : false,
        // produce sourcemaps for debug builds
        sourcemap: !!process.env.TAURI_DEBUG,
        // Mermaid is lazy-loaded and remains a relatively large async chunk.
        chunkSizeWarningLimit: 650,
    },
})
