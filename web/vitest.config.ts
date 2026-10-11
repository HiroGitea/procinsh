import { resolve } from "node:path";
import { defineConfig } from "vitest/config";

export default defineConfig({
  server: {
    fs: {
      allow: [
        resolve(import.meta.dirname, ".."),
        resolve(import.meta.dirname, "../../../glyph3d-js"),
      ],
    },
  },
  resolve: {
    dedupe: ["three"],
  },
  test: {
    environment: "node",
    include: ["tests/**/*.test.ts"],
    includeSource: ["src/**/*.ts"],
  },
});
