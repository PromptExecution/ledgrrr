import { defineConfig } from 'vite';
import { fileURLToPath } from 'node:url';

// Resolve relative to this file (ESM-safe — no __dirname available here).
const here = (path: string): string => fileURLToPath(new URL(path, import.meta.url));

export default defineConfig({
  base: './',
  worker: { format: 'es' },
  optimizeDeps: {
    exclude: ['@huggingface/transformers'],
  },
  build: {
    rollupOptions: {
      // Multi-page build: the pipeline-viz app (index.html) and the
      // billing/cost dashboard (billing.html) as sibling entry points.
      input: {
        main: here('./index.html'),
        billing: here('./billing.html'),
      },
    },
  },
});
