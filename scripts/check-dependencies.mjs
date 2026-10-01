#!/usr/bin/env node
// Reject local dependency edges outside the architecture allowlist.

import { readdir, readFile } from "node:fs/promises";

if (process.argv.includes("--typescript")) {
  const allowed = new Map([
    ["@retirement-calculator/contracts", new Set()],
    ["@retirement-calculator/domain", new Set()],
  ]);
  const directories = await readdir(new URL("../packages/", import.meta.url), { withFileTypes: true });
  const manifests = await Promise.all(directories
    .filter((entry) => entry.isDirectory())
    .map(async (entry) => JSON.parse(await readFile(new URL(`../packages/${entry.name}/package.json`, import.meta.url), "utf8"))));
  const workspaceNames = new Set(manifests.map((manifest) => manifest.name));
  const errors = [];
  for (const manifest of manifests) {
    const expected = allowed.get(manifest.name);
    if (expected === undefined) {
      errors.push(`unexpected TypeScript workspace package: ${manifest.name}`);
      continue;
    }
    const dependencyNames = Object.keys({
      ...manifest.dependencies,
      ...manifest.devDependencies,
      ...manifest.optionalDependencies,
      ...manifest.peerDependencies,
    });
    const unexpected = dependencyNames.filter((name) => workspaceNames.has(name) && !expected.has(name));
    if (unexpected.length > 0) errors.push(`${manifest.name}: forbidden local dependencies: ${unexpected.sort().join(", ")}`);
  }
  for (const expected of allowed.keys()) {
    if (!workspaceNames.has(expected)) errors.push(`missing TypeScript workspace package: ${expected}`);
  }
  if (errors.length > 0) {
    console.error("TypeScript dependency policy: FAILED");
    console.error(errors.join("\n"));
    process.exit(1);
  }
  console.log("TypeScript dependency policy: PASS");
  for (const name of [...allowed.keys()].sort()) console.log(`  ${name} -> (none)`);
  process.exit(0);
}

// Legacy Rust boundary check remains until the completed rewrite removes Rust.

let input = "";
for await (const chunk of process.stdin) input += chunk;
const metadata = JSON.parse(input);
const workspace = new Set(metadata.packages.map((pkg) => pkg.name));
const allowed = new Map([
  ["domain", new Set()],
  ["application", new Set(["domain"])],
  ["api-contract", new Set()],
  ["adapters-memory", new Set(["application", "domain"])],
  ["server", new Set(["adapters-memory", "api-contract", "application", "domain"])],
  ["web", new Set(["api-contract"])],
]);

const unexpectedPackages = [...workspace].filter((pkg) => !allowed.has(pkg));
const actual = new Map(metadata.packages.map((pkg) => [
  pkg.name,
  new Set(pkg.dependencies.filter((dependency) => workspace.has(dependency.name)).map((dependency) => dependency.name)),
]));

const errors = [];
if (unexpectedPackages.length) {
  errors.push(`unexpected workspace packages: ${unexpectedPackages.sort().join(", ")}`);
}
for (const [pkg, expected] of allowed) {
  const unexpected = [...(actual.get(pkg) ?? [])].filter((dependency) => !expected.has(dependency));
  if (unexpected.length) errors.push(`${pkg}: forbidden local dependencies: ${unexpected.sort().join(", ")}`);
}

if (errors.length) {
  console.error("dependency policy: FAILED");
  console.error(errors.join("\n"));
  process.exit(1);
}

console.log("dependency policy: PASS");
for (const pkg of [...allowed.keys()].sort()) {
  const edges = [...(actual.get(pkg) ?? [])].sort().join(", ") || "(none)";
  console.log(`  ${pkg} -> ${edges}`);
}
