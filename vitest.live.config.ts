import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["tests/live/**/*.live.test.ts"],
    testTimeout: 60_000,
    hookTimeout: 10_000,
    fileParallelism: false,
  },
});
