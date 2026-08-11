import { resolve } from 'path';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

const entries = {
  ai_monitor: resolve(__dirname, 'src/entries/ai_monitor.tsx'),
  nl_query: resolve(__dirname, 'src/entries/nl_query.tsx'),
  llm_eval_lab: resolve(__dirname, 'src/entries/llm_eval_lab.tsx'),
  dispatch_board_ai: resolve(__dirname, 'src/entries/dispatch_board_ai.tsx'),
  dashboard_ai_widget: resolve(__dirname, 'src/entries/dashboard_ai_widget.tsx'),
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
