import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import { resolve } from 'path';
import { PRODUCTION_HTML_ENTRIES } from './src/shared/production-html-entries';

const apiProxyTarget = process.env.VITE_API_PROXY_TARGET || 'https://localhost:18443';

const htmlEntries: Record<string, string> = {};
for (const name of PRODUCTION_HTML_ENTRIES) {
  htmlEntries[name] = resolve(__dirname, `${name}.html`);
}

export default defineConfig({
  base: '/frontend/',
  plugins: [
    vue({
      template: {
        transformAssetUrls: {
          img: [],
        },
      },
    }),
  ],
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
    },
  },
  server: {
    host: '0.0.0.0',
    port: 3000,
    strictPort: false,
    cors: true,
    open: false,
    proxy: {
      '/api': {
        target: apiProxyTarget,
        changeOrigin: true,
        secure: false,
      },
    },
  },
  build: {
    rollupOptions: {
      input: htmlEntries,
    },
  },
  preview: {
    host: '0.0.0.0',
    port: 3001,
    cors: true,
  },
});
