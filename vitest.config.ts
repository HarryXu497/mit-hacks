import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/*.test.ts", "server/**/*.test.ts"],
    exclude: ["tests/live/**", "node_modules/**", "dist/**", ".claude/**"],
  },
});
