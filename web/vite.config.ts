import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

const api = process.env.MICP_API_ORIGIN ?? "http://127.0.0.1:8081";

export default defineConfig({
  plugins: [react()],
  server: {
    host: "127.0.0.1",
    port: 5173,
    proxy: {
      "/health": api,
      "/v1": api,
    },
  },
  preview: {
    host: "127.0.0.1",
    port: 4173,
    proxy: {
      "/health": api,
      "/v1": api,
    },
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
  },
});
