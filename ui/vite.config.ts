import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";

// clearScreen false + fixed port: Tauri's devUrl points here.
export default defineConfig({
  plugins: [svelte(), tailwindcss()],
  clearScreen: false,
  server: {
    host: "0.0.0.0",
    port: Number(process.env.VITE_PORT ?? 5173),
    strictPort: true,
    allowedHosts: ["localhost", ".e2b.app"],
    proxy: { "/api": `http://127.0.0.1:${process.env.BONAPARTE_API_PORT ?? 4317}` },
  },
  build: { target: "es2022" },
});
