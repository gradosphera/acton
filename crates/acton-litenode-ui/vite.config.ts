import path from "node:path"

import react from "@vitejs/plugin-react"
import {defineConfig} from "vite"
import {nodePolyfills} from "vite-plugin-node-polyfills"

const liteNodeTarget = process.env.VITE_LITENODE_PROXY_TARGET || "http://localhost:3010"

export default defineConfig({
  plugins: [
    react(),
    nodePolyfills({
      include: ["buffer", "path"],
      globals: {
        Buffer: true,
      },
    }),
  ],
  resolve: {
    alias: {
      "@acton/shared-ui": path.resolve(import.meta.dirname, "../acton-shared-ui/src"),
      "@": path.resolve(import.meta.dirname, "../acton-shared-ui/src"),
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
  server: {
    port: 3006,
    proxy: {
      "/api": {
        target: liteNodeTarget,
        changeOrigin: true,
      },
      "/admin": {
        target: liteNodeTarget,
        changeOrigin: true,
      },
      "/emulate": {
        target: liteNodeTarget,
        changeOrigin: true,
      },
    },
  },
})
