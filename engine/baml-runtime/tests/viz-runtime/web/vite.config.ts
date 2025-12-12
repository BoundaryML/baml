import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 4173,
    fs: {
      allow: [
        __dirname,
        path.resolve(__dirname, "../snapshots"),
      ],
    },
  },
});
