import { defineConfig } from 'vite'

export default defineConfig({
  server: {
    port: 5173,
    proxy: {
      '/api': 'http://localhost:7879',
      '/events': 'http://localhost:7879',
    },
  },
  build: {
    outDir: '../src/assets/dist',
    emptyOutDir: true,
  },
})
