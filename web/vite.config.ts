import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Dev proxy target: a running boompid. Defaults to a local `make sim` +
// boompid on :3001; point at the dev box with e.g.
//   BOOMPI_HOST=http://boompi-dev-2.local:8080 pnpm dev
const target = process.env.BOOMPI_HOST ?? "http://127.0.0.1:3001";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    proxy: {
      "/api": { target, changeOrigin: true },
      "/art": { target, changeOrigin: true },
      "/ws": { target, changeOrigin: true, ws: true },
    },
  },
  build: {
    // Committed to git and embedded into boompid via rust-embed, so cargo
    // and Buildroot builds never need a Node toolchain (`make web` refreshes).
    outDir: "dist",
    assetsDir: "assets",
  },
});
