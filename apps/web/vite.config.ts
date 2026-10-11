import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

const server = process.env.HOLLOWRITE_SERVER ?? "http://127.0.0.1:7700";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@content": fileURLToPath(new URL("../../content", import.meta.url)),
    },
  },
  server: {
    port: 5173,
    strictPort: true,
    fs: { allow: ["../.."] },
    proxy: {
      "/api": { target: server, changeOrigin: false },
      "/ws": { target: server.replace(/^http/, "ws"), ws: true, changeOrigin: false },
    },
  },
  build: {
    target: "es2022",
    sourcemap: true,
  },
  test: {
    environment: "node",
  },
});
