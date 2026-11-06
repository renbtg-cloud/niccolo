import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
export default defineConfig({
  plugins: [react()],
  build: {
    sourcemap: true,
    rollupOptions: { output: { entryFileNames: 'assets/app.js', chunkFileNames: 'assets/chunks/[name].js', assetFileNames: (a) => a.names?.some((n)=>n.endsWith('.css')) ? 'assets/app.css' : 'assets/[name][extname]' } }
  },
  test: { environment: 'node' }
});
