#!/usr/bin/env bash
# Oracle de bissection : 0 = compression < 50 %, 1 = régression, 125 = essai impossible.
# Copier ce script hors de l'arbre avant git bisect run.
# Les journaux restent dans LMR_REGRESSION_WORK_DIR (temporaire par défaut).
set -uo pipefail

root=$(git rev-parse --show-toplevel) || exit 125
cd "$root" || exit 125
work=${LMR_REGRESSION_WORK_DIR:-${TMPDIR:-/tmp}/lm-resizer-regression-grep}
mkdir -p "$work" || exit 125
work=$(cd "$work" && pwd) || exit 125
trial=$(mktemp -d "$work/trial.XXXXXX") || exit 125
mkdir "$trial/home" || exit 125
revision=$(git rev-parse --short HEAD) || exit 125

# HOME réel pour Cargo/rustup seulement ; aucune configuration utilisateur pour exec.
# Même cible pour tous les essais afin de réutiliser les dépendances compilées.
if ! cargo build --release -j 8 --target-dir "$work/target" >"$trial/build.log" 2>&1; then
    echo "$revision : compilation impossible ; journal : $trial/build.log" >&2
    tail -n 30 "$trial/build.log" >&2
    exit 125
fi
if ! env -i PATH="$PATH" HOME="$trial/home" LC_ALL=C \
    bash -c 'grep -rn fn crates/lm-resizer-core/src/transforms' \
    >"$trial/raw.txt" 2>"$trial/raw.stderr"; then
    echo "$revision : grep impossible ; journal : $trial/raw.stderr" >&2
    exit 125
fi
if ! env -i PATH="$PATH" HOME="$trial/home" LC_ALL=C \
    "$work/target/release/lm-resizer" exec -- \
    bash -c 'grep -rn fn crates/lm-resizer-core/src/transforms' \
    >"$trial/output.txt" 2>"$trial/exec.stderr"; then
    echo "$revision : exec impossible ; journal : $trial/exec.stderr" >&2
    exit 125
fi
raw=$(wc -c <"$trial/raw.txt")
output=$(wc -c <"$trial/output.txt")
if (( raw == 0 )); then
    echo "$revision : sortie brute vide" >&2
    exit 125
fi
printf '%s : %d -> %d octets ; journaux : %s\n' "$revision" "$raw" "$output" "$trial"
if (( output * 2 < raw )); then
    exit 0
fi
exit 1
