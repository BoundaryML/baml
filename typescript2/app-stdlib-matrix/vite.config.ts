import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

// Relative base so the built page works from any path — a static host, a
// subdirectory, or opened directly — without a rebuild.
export default defineConfig({
  base: "./",
  plugins: [tailwindcss()],
  build: { outDir: "dist", emptyOutDir: true },
});
