import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import test from "node:test";

const approvedPackages = [
  "domain",
  "application",
  "api-contract",
  "adapters-memory",
  "server",
  "web",
];

test("rejects an unapproved workspace package", () => {
  const metadata = {
    packages: [...approvedPackages, "unapproved-package"].map((name) => ({
      name,
      dependencies: [],
    })),
  };
  const result = spawnSync(process.execPath, ["scripts/check-dependencies.mjs"], {
    cwd: new URL("..", import.meta.url),
    encoding: "utf8",
    input: JSON.stringify(metadata),
  });

  assert.equal(result.status, 1);
  assert.match(result.stderr, /dependency policy: FAILED/);
  assert.match(result.stderr, /unexpected workspace packages: unapproved-package/);
});
