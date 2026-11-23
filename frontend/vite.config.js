import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
export default defineConfig({
  plugins: [react()],
  build: {
    // The local MVP ships one inspectable bundle but not its complete source tree.
    sourcemap: false,
    rollupOptions: { output: { entryFileNames: 'assets/app.js', chunkFileNames: 'assets/chunks/[name].js', assetFileNames: (a) => a.names?.some((n)=>n.endsWith('.css')) ? 'assets/app.css' : 'assets/[name][extname]' } }
  },
  test: { environment: 'node' }
});
