import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

const target = process.env.DOGUBAKO_API ?? 'http://127.0.0.1:8420';

export default defineConfig({
  plugins: [react()],
  base: './',
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    target: 'es2021',
    chunkSizeWarningLimit: 400,
  },
  server: {
    port: 5173,
    proxy: {
      '/api': { target, changeOrigin: false },
      '/mcp': { target, changeOrigin: false },
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    include: ['src/**/*.test.{ts,tsx}'],
  },
});
