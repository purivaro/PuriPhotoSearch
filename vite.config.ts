import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

// Tauri ต้องการ dev port คงที่ และไม่ต้อง reload เมื่อ Rust เปลี่ยน
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) } },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**", "**/screenshots/**"] },
  },
  build: { target: "safari15", chunkSizeWarningLimit: 900 },
  test: { environment: "node", include: ["src/**/*.test.ts"] },
});
