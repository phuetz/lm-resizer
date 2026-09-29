#!/usr/bin/env sh
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
package_json="$root/packages/wasm/package.json"
workflow="$root/.github/workflows/publish-wasm.yml"
evidence="$root/dist/release-evidence.json"

[ -f "$package_json" ] || { printf >&2 '%s\n' "missing $package_json"; exit 1; }
[ -f "$workflow" ] || { printf >&2 '%s\n' "missing $workflow"; exit 1; }
[ -f "$evidence" ] || { printf >&2 '%s\n' "missing $evidence; run scripts/check-release.sh first"; exit 1; }

name="$(node -e "console.log(require('$package_json').name)")"
version="$(node -e "console.log(require('$package_json').version)")"
npm_version="$(node -e "console.log(require('$evidence').npm_version)")"
tarballs="$(node -e "console.log((require('$evidence').wasm_tarballs || []).join(','))")"

[ "$name" = "@phuetz/lm-resizer" ] || { printf >&2 '%s\n' "unexpected npm package name: $name"; exit 1; }
[ -n "$version" ] || { printf >&2 '%s\n' "missing npm package version"; exit 1; }
[ "$npm_version" = "$version" ] || {
  printf >&2 '%s\n' "release evidence npm_version $npm_version does not match package.json $version"
  exit 1
}
[ -n "$tarballs" ] || { printf >&2 '%s\n' "release evidence does not list a WASM tarball"; exit 1; }

ROOT="$root" node - <<'NODE'
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const root = process.env.ROOT;
const read = (name) => JSON.parse(fs.readFileSync(path.join(root, name), 'utf8'));
const pkg = read('packages/wasm/package.json');
const plugin = read('.claude-plugin/plugin.json');
const marketplace = read('.claude-plugin/marketplace.json');
const evidence = read('dist/release-evidence.json');
const fail = (message) => { console.error(message); process.exit(1); };
const lm = marketplace.plugins.find((item) => item.name === 'lm-resizer');
if (!lm || plugin.version !== pkg.version || lm.version !== pkg.version)
  fail('plugin, marketplace and npm package versions do not match');
if (evidence.version !== pkg.version || Object.values(evidence.cargo_versions || {}).some((version) => version !== pkg.version))
  fail('release evidence Cargo versions do not match npm package');
const artifacts = [...(evidence.wasm_tarballs || []), ...(evidence.binary_archives || [])];
if (!evidence.binary_archives?.length) fail('release evidence does not list a platform binary archive');
const sumsPath = path.join(root, 'dist/SHA256SUMS');
if (!fs.existsSync(sumsPath)) fail('missing dist/SHA256SUMS');
const sums = new Map(fs.readFileSync(sumsPath, 'utf8').trim().split('\n').filter(Boolean).map((line) => {
  const match = /^([0-9a-f]{64})  (.+)$/.exec(line);
  if (!match) fail(`invalid checksum line: ${line}`);
  return [match[2], match[1]];
}));
for (const name of artifacts) {
  if (!name.includes(pkg.version)) fail(`stale artifact version: ${name}`);
  const file = path.join(root, 'dist', name);
  if (!fs.existsSync(file)) fail(`missing release artifact: ${name}`);
  const digest = crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
  if (sums.get(name) !== digest) fail(`missing or invalid SHA-256 for ${name}`);
}
NODE

for required in \
  "environment: npm-production" \
  "id-token: write" \
  "NPM_TRUSTED_PUBLISHING" \
  "NPM_PROVENANCE" \
  "scripts/publish-wasm.sh"
do
  grep -q "$required" "$workflow" || {
    printf >&2 '%s\n' "publish workflow missing: $required"
    exit 1
  }
done

node -e "if (!(require('$evidence').release_checks || []).includes('scripts/smoke-proxy-preview.sh')) process.exit(1)" || {
  printf >&2 '%s\n' "release evidence does not list proxy smoke check"
  exit 1
}

if [ "${NPM_TRUSTED_PUBLISHING:-}" = "1" ]; then
  auth_status="trusted-publishing-env"
elif [ -n "${NPM_TOKEN:-}" ]; then
  auth_status="npm-token-env"
else
  auth_status="external-approval-required"
fi

node - <<NODE
console.log(JSON.stringify({
  package: "$name",
  version: "$version",
  workflow: ".github/workflows/publish-wasm.yml",
  environment: "npm-production",
  evidence: "dist/release-evidence.json",
  tarballs: "$tarballs".split(",").filter(Boolean),
  auth_status: "$auth_status",
  ready_for_manual_publish_after_approval: true,
  external_requirements: [
    "Configure npm trusted publishing for the GitHub repository or provide NPM_TOKEN",
    "Protect the npm-production GitHub environment with maintainer approval",
    "Run the Publish WASM Package workflow with matching confirm_package and confirm_version"
  ]
}, null, 2));
NODE
