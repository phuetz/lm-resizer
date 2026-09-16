#!/usr/bin/env bash
# Install the Grok-authored skill. Does not delete extra files in dest.
# On any content conflict, skip the whole copy unless --force (then backup dest).
set -euo pipefail

SKILL_NAME="lm-resizer"
usage() {
  echo "usage: $0 [--target grok|codex] [--dest DIR] [--force]" >&2
}

TARGET="grok"
FORCE=0
DEST_OVERRIDE=""
while [ $# -gt 0 ]; do
  case "$1" in
    --target)
      TARGET="${2:?--target needs a value}"
      shift 2
      ;;
    --target=*)
      TARGET="${1#*=}"
      shift
      ;;
    --dest)
      DEST_OVERRIDE="${2:?--dest needs a value}"
      shift 2
      ;;
    --dest=*)
      DEST_OVERRIDE="${1#*=}"
      shift
      ;;
    --force)
      FORCE=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

case "$TARGET" in
  grok) BASE="${GROK_SKILLS_DIR:-$HOME/.grok/skills}" ;;
  codex) BASE="${CODEX_SKILLS_DIR:-$HOME/.codex/skills}" ;;
  *)
    echo "unsupported --target '$TARGET' (supported: grok, codex)" >&2
    exit 2
    ;;
esac

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/skills/grok/$SKILL_NAME"
DEST="${DEST_OVERRIDE:-$BASE/$SKILL_NAME}"

if [ ! -d "$SRC" ]; then
  echo "missing source skill: $SRC" >&2
  exit 1
fi

mkdir -p "$DEST"

conflicts=0
while IFS= read -r rel; do
  [ -z "$rel" ] && continue
  if [ -e "$DEST/$rel" ] && ! cmp -s "$SRC/$rel" "$DEST/$rel"; then
    echo "conflict: $DEST/$rel"
    conflicts=1
  fi
done < <(cd "$SRC" && find . -type f | sed 's|^\./||' | sort)

if [ "$conflicts" -eq 1 ] && [ "$FORCE" -ne 1 ]; then
  echo "skip: dest has personalized files (pass --force to backup dest and copy)"
  exit 0
fi

if [ "$FORCE" -eq 1 ] && [ -d "$DEST" ]; then
  bak="${DEST}.bak.$(date -u +%Y%m%dT%H%M%SZ)"
  mkdir -p "$bak"
  cp -a "$DEST"/. "$bak"/ 2>/dev/null || true
  echo "backup: $bak"
fi

copied=0
while IFS= read -r rel; do
  [ -z "$rel" ] && continue
  mkdir -p "$DEST/$(dirname "$rel")"
  if [ ! -e "$DEST/$rel" ] || [ "$FORCE" -eq 1 ]; then
    cp "$SRC/$rel" "$DEST/$rel"
    copied=1
  fi
done < <(cd "$SRC" && find . -type f | sed 's|^\./||' | sort)

if [ "$copied" -eq 1 ]; then
  echo "installed: $DEST (target=$TARGET)"
else
  echo "already installed: $DEST (target=$TARGET)"
fi
