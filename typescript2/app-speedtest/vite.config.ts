import path from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  server: {
    port: 3333,
    proxy: {
      "/api": {
        target: `http://localhost:${process.env.SPEEDTEST_API_PORT || 4444}`,
      },
    },
  },
});
