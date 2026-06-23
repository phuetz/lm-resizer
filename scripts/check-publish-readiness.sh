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

[ "$name" = "@lm-resizer/wasm" ] || { printf >&2 '%s\n' "unexpected npm package name: $name"; exit 1; }
[ -n "$version" ] || { printf >&2 '%s\n' "missing npm package version"; exit 1; }
[ "$npm_version" = "$version" ] || {
  printf >&2 '%s\n' "release evidence npm_version $npm_version does not match package.json $version"
  exit 1
}
[ -n "$tarballs" ] || { printf >&2 '%s\n' "release evidence does not list a WASM tarball"; exit 1; }

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
