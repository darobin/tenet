
import { defineConfig } from 'vite';
import { litTemplateLoader } from '@mordech/vite-lit-loader';

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  clearScreen: false,
  plugins: [litTemplateLoader()],
  server: {
    host: host || false,
    port: 5173,
    strictPort: true,
    hmr: host ? { protocol: 'ws', host, port: 5183 } : undefined,
    watch: { ignored: ['**/src-tauri/**', '**/tiles/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_*'],
  build: {
    target: 'baseline-widely-available',
    minify: !process.env.TAURI_ENV_DEBUG ? 'esbuild' : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});
