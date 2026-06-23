#!/usr/bin/env sh
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
dist="$root/dist"
mkdir -p "$dist"

version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -n 1)"
core_version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/crates/lm-resizer-core/Cargo.toml" | head -n 1)"
wasm_version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/crates/lm-resizer-wasm/Cargo.toml" | head -n 1)"
npm_version="$(node -e "console.log(require('$root/packages/wasm/package.json').version)")"

ROOT="$root" DIST="$dist" VERSION="$version" CORE_VERSION="$core_version" WASM_VERSION="$wasm_version" NPM_VERSION="$npm_version" node - <<'NODE'
const fs = require("node:fs");
const path = require("node:path");
const root = process.env.ROOT;
const dist = process.env.DIST;
const tarballs = fs.existsSync(dist)
  ? fs.readdirSync(dist).filter((name) => name.endsWith(".tgz"))
  : [];
const binaryArchives = fs.existsSync(dist)
  ? fs.readdirSync(dist).filter((name) => name.endsWith(".zip") || name.endsWith(".tar.gz"))
  : [];
const fixturesDir = path.join(root, "fixtures", "provider-cache");
const fixtures = fs.existsSync(fixturesDir)
  ? fs.readdirSync(fixturesDir).filter((name) => name.endsWith(".json"))
  : [];
const checksums = fs.existsSync(path.join(dist, "SHA256SUMS")) ? ["SHA256SUMS"] : [];
const evidence = {
  generated_at_utc: new Date().toISOString(),
  version: process.env.VERSION,
  cargo_versions: {
    root: process.env.VERSION,
    core: process.env.CORE_VERSION,
    wasm: process.env.WASM_VERSION,
  },
  npm_version: process.env.NPM_VERSION,
  wasm_tarballs: tarballs,
  binary_archives: binaryArchives,
  provider_cache_fixtures: fixtures,
  checksums,
  release_checklist: "docs/RELEASE.md",
  contribution_docs: [
    "CONTRIBUTING.md",
    "SECURITY.md",
  ],
  github_templates: [
    ".github/ISSUE_TEMPLATE/provider-fixture.yml",
    ".github/ISSUE_TEMPLATE/custom-filter.yml",
    ".github/dependabot.yml",
  ],
  release_checks: [
    "cargo fmt --check",
    "cargo test --release",
    "cargo check --release",
    "cargo check --release --examples",
    "cargo build --release",
    "scripts/smoke-proxy-preview.sh",
    "scripts/check-wasm-package.sh",
    "scripts/publish-wasm.sh --dry-run",
    "scripts/generate-checksums.sh",
    "scripts/check-publish-readiness.sh",
  ],
  publish_commands: {
    dry_run: "scripts/publish-wasm.sh --dry-run",
    real_publish: "scripts/publish-wasm.sh",
  },
  publication_requires: [
    "npm trusted publishing for .github/workflows/publish-wasm.yml or NPM_TOKEN/npm login fallback",
    "release approval for npm publish --access public --provenance",
  ],
};
fs.writeFileSync(path.join(dist, "release-evidence.json"), JSON.stringify(evidence, null, 2));
NODE

printf '%s\n' "Release evidence written to $dist/release-evidence.json"
