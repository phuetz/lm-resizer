#!/usr/bin/env sh
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
dry_run=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) dry_run=1 ;;
    *)
      printf >&2 '%s\n' "unknown option: $arg"
      printf >&2 '%s\n' "usage: scripts/publish-wasm.sh [--dry-run]"
      exit 2
      ;;
  esac
done

"$root/scripts/package-wasm.sh"

temp_npmrc=""
if [ "$dry_run" -eq 1 ]; then
  printf '%s\n' "Running npm publish dry-run; authentication is not required"
elif [ -n "${NPM_TOKEN:-}" ]; then
  temp_npmrc="${TMPDIR:-/tmp}/lm-resizer-npm-$$.npmrc"
  printf '%s' "//registry.npmjs.org/:_authToken=$NPM_TOKEN" > "$temp_npmrc"
  export NPM_CONFIG_USERCONFIG="$temp_npmrc"
elif [ "${NPM_TRUSTED_PUBLISHING:-}" = "1" ]; then
  printf '%s\n' "Using npm trusted publishing / OIDC"
else
  npm whoami >/dev/null || {
    printf >&2 '%s\n' "npm is not authenticated. Set NPM_TOKEN, enable NPM_TRUSTED_PUBLISHING=1, or run npm login."
    exit 1
  }
fi

cd "$root/packages/wasm"
trap 'if [ -n "$temp_npmrc" ]; then rm -f "$temp_npmrc"; fi' EXIT
if [ "$dry_run" -eq 1 ]; then
  npm publish --access public --dry-run
elif [ "${NPM_PROVENANCE:-}" = "1" ]; then
  npm publish --access public --provenance
else
  npm publish --access public
fi
