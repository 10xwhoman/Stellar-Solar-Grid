import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // Use node environment (no DOM)
    environment: "node",
    // Only run files named *.integration.test.ts under tests/ — the other
    // files there are plain tsx scripts (scrub, cors, processMessage) that
    // are not vitest suites and should continue to be invoked via
    // `npm run test:*`. Colocated `src/**/*.test.ts` files (e.g.
    // lib/dateUtils.test.ts, lib/creditScoring.test.ts) are genuine vitest
    // suites and must also be picked up by the bare `vitest`/`npm test`
    // invocation — see #1003 investigation.
    include: ["tests/**/*.integration.test.ts", "src/**/*.test.ts"],
    // 30-second timeout for integration tests
    testTimeout: 30_000,
  },
});
