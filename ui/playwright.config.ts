import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: "./tests",
  testMatch: "**/*.spec.ts",
  fullyParallel: false,
  workers: 1,
  timeout: 60000,
  expect: { timeout: 15000 },
  reporter: "list",
  use: {
    baseURL: "http://127.0.0.1:5183",
    viewport: { width: 1440, height: 900 },
    acceptDownloads: true,
  },
  webServer: {
    command: "npm run studio",
    url: "http://127.0.0.1:5183",
    reuseExistingServer: false,
    timeout: 180000,
    env: { BONAPARTE_API_PORT: "4318", VITE_PORT: "5183" },
  },
});
