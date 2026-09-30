#!/usr/bin/env node
// Reject local Cargo dependency edges outside the architecture allowlist.

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
