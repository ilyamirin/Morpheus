import { defineConfig } from "vite";

export default defineConfig({
  build: {
    emptyOutDir: false,
    lib: {
      entry: "crates/morpheus-server/ui/src/app.js",
      name: "MorpheusUi",
      formats: ["iife"],
      fileName: () => "app.bundle.js"
    },
    outDir: "crates/morpheus-server/ui/assets"
  }
});
