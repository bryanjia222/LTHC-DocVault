import { defineConfig } from "vitest/config";
import vue from "@vitejs/plugin-vue";

/*
 * Standalone Vitest config, separate from vite.config.ts so the Tauri build is
 * untouched. jsdom is ready for component tests; pure-helper tests run fine in
 * it too. `globals: false` is deliberate - tests import { describe, it, ... }
 * from "vitest" explicitly, so the production tsconfig needs no "types" change
 * and `vue-tsc --noEmit` keeps type-checking test files without extra setup.
 */
export default defineConfig({
  plugins: [vue()],
  test: {
    environment: "jsdom",
    globals: false,
    setupFiles: ["src/test/setup.ts"],
    include: ["src/**/*.{test,spec}.ts"],
  },
});
