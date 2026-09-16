import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'node:path'

// Tauri expects a fixed port and no clearScreen so its own logs stay visible.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  resolve: { alias: { '@': path.resolve(__dirname, 'src') } },
  build: { target: 'esnext', sourcemap: true },
})
