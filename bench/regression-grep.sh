#!/usr/bin/env bash
# Oracle historique : 0 = < 50 % ET toutes les lignes visibles; 1 = perte, plantage ou gain insuffisant.
# 125 est réservé à une compilation ou une entrée indisponible.
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
if ! cargo build --release -j 8 --bin lm-resizer --message-format=json-render-diagnostics --target-dir "$work/target" >"$trial/build.jsonl" 2>"$trial/build.log"; then
    echo "$revision : compilation impossible ; journal : $trial/build.log" >&2
    tail -n 30 "$trial/build.log" >&2
    exit 125
fi
binary=$(python3 - "$trial/build.jsonl" <<'PYBUILD'
import json, sys
messages = [json.loads(line) for line in open(sys.argv[1])]
artifacts = [m['executable'] for m in messages if m.get('reason') == 'compiler-artifact' and m.get('target', {}).get('name') == 'lm-resizer' and m.get('executable')]
if len(artifacts) != 1:
    raise SystemExit(1)
print(artifacts[0])
PYBUILD
) || exit 1
if ! env -i PATH="$PATH" HOME="$trial/home" LC_ALL=C \
    bash -c 'grep -rn fn crates/lm-resizer-core/src/transforms' \
    >"$trial/raw.txt" 2>"$trial/raw.stderr"; then
    echo "$revision : grep impossible ; journal : $trial/raw.stderr" >&2
    exit 125
fi
if ! env -i PATH="$PATH" HOME="$trial/home" LC_ALL=C \
    "$binary" exec --json -- \
    bash -c 'grep -rn fn crates/lm-resizer-core/src/transforms' \
    >"$trial/output.json" 2>"$trial/exec.stderr"; then
    echo "$revision : exec en échec ; journal : $trial/exec.stderr" >&2
    exit 1
fi
python3 - "$trial/raw.txt" "$trial/output.json" "$trial/output.txt" <<'PYFACTS'
from collections import Counter
import json, pathlib, sys
raw = pathlib.Path(sys.argv[1]).read_text()
report = json.loads(pathlib.Path(sys.argv[2]).read_text())
output = report['output']
pathlib.Path(sys.argv[3]).write_text(output)
lost = Counter(raw.splitlines()) - Counter(output.splitlines())
if lost:
    print(f"faits absents dans la vue: {sum(lost.values())}", file=sys.stderr)
    raise SystemExit(1)
if report['exit_code'] != 0:
    raise SystemExit(1)
PYFACTS
facts_status=$?
if (( facts_status != 0 )); then exit 1; fi
raw=$(wc -c <"$trial/raw.txt")
output=$(wc -c <"$trial/output.txt")
if (( raw == 0 )); then exit 125; fi
printf '%s : %d -> %d octets ; journaux : %s\n' "$revision" "$raw" "$output" "$trial"
if (( output * 2 < raw )); then exit 0; fi
exit 1
