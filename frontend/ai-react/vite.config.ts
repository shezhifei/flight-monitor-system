import { resolve } from 'path';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

const entries = {
  ai_monitor: resolve(__dirname, 'src/entries/ai_monitor.tsx'),
  nl_query: resolve(__dirname, 'src/entries/nl_query.tsx'),
  dispatch_board_ai: resolve(__dirname, 'src/entries/dispatch_board_ai.tsx'),
};

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
    },
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
  },
  base: '/frontend/static/ai/',
  build: {
    manifest: 'manifest.json',
    outDir: resolve(__dirname, '../static/ai'),
    emptyOutDir: true,
    sourcemap: false,
    target: 'es2021',
    rollupOptions: {
      input: entries,
      output: {
        entryFileNames: 'assets/[name]-[hash].js',
        chunkFileNames: 'assets/chunk-[name]-[hash].js',
        assetFileNames: 'assets/[name]-[hash][extname]',
      },
    },
  },
});
