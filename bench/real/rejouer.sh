#!/usr/bin/env bash
# Rejeu en une étape du banc de 61 captures (médiane, moyenne, vues, codes de sortie).
#
#   bench/real/rejouer.sh [--sans-headroom] [--repetitions N] [--work DOSSIER]
#                         [--captures N] [--seuil-mediane P] [--seuil-moyenne P]
#
# Ce que fait le script, dans l'ordre, sans rien écrire hors de target/ (hors le cache habituel de
# cargo, CARGO_HOME, et le dossier cible de cargo s'il est redirigé) :
#   1. un environnement Python isolé avec tiktoken 0.14.0 (et Headroom 0.39.1 + ONNX Runtime
#      1.24.4, sauf --sans-headroom) ;
#   2. l'oracle de comparaison, construit depuis l'archive amont épinglée dans
#      bench/rtk-parity/reference.json (SHA-256 vérifié, arbre contrôlé avant et après) ;
#   3. le binaire lm-resizer de CE checkout (ou LM_RESIZER_BIN pour en rejouer un autre) ;
#   4. bench/real/parity_rtk.py sur bench/rtk-parity/corpus.json.gz, dans un dossier neuf ;
#   5. une synthèse lue dans results.json.
#
# Réseau nécessaire la première fois (archive amont, crates, paquets Python).
# Sortie du script : 0 seulement si les 61 captures sont mesurées, si la médiane et la moyenne de
# LM Resizer atteignent les seuils de la 0.2.6 (4,87 % et 28,39 %, modifiables), si les 61 bruts se récupèrent ET si les 61 codes du producteur sont conservés (voir
# synthese_rejeu.py). L'égalité stricte des vues n'est pas une condition : parity_rtk.py sort en 1
# tant que des vues diffèrent (écarts publiés dans bench/native/README.md) ; le nombre exact est
# affiché.
set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
qa_dir="$repo_dir/target/rejeu"
headroom=1
repetitions=3
work=""
seuils=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --sans-headroom) headroom=0; shift ;;
    --repetitions) repetitions="${2:?--repetitions demande un entier}"; shift 2 ;;
    --work) work="${2:?--work demande un dossier}"; shift 2 ;;
    --captures) seuils+=(--captures "${2:?--captures demande un entier}"); shift 2 ;;
    --seuil-mediane) seuils+=(--mediane-min "${2:?--seuil-mediane demande un pourcentage}"); shift 2 ;;
    --seuil-moyenne) seuils+=(--moyenne-min "${2:?--seuil-moyenne demande un pourcentage}"); shift 2 ;;
    -h|--help) sed -n '2,24p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "usage: bench/real/rejouer.sh [--sans-headroom] [--repetitions N] [--work DOSSIER] [--captures N] [--seuil-mediane P] [--seuil-moyenne P]" >&2; exit 2 ;;
  esac
done
# parity_rtk.py refuse un dossier existant : chaque rejeu garde ses propres chemins.
work="${work:-$qa_dir/rejeu-$(date +%Y%m%d-%H%M%S)-$$}"
mkdir -p "$qa_dir"
cd "$repo_dir"
# Tout ce que les outils Python écrivent reste sous target/ : sans cela `uv venv` crée son cache
# dans ~/.cache/uv et échoue quand HOME n'est pas inscriptible (contre-revue du 08/10/2026), et
# tiktoken dépose ses encodages dans le dossier temporaire du système.
export UV_CACHE_DIR="$qa_dir/uv-cache" PIP_CACHE_DIR="$qa_dir/pip-cache" TIKTOKEN_CACHE_DIR="$qa_dir/tiktoken-cache"

for outil in python3 cargo git curl; do
  command -v "$outil" >/dev/null || { echo "outil manquant : $outil" >&2; exit 2; }
done

if [[ "$headroom" == 1 ]]; then echo "== 1/5 Python isolé (tiktoken, Headroom)"; else echo "== 1/5 Python isolé (tiktoken)"; fi
venv="$qa_dir/venv"
if [[ ! -x "$venv/bin/python" ]]; then
  if command -v uv >/dev/null 2>&1; then uv venv --python python3 "$venv"; else python3 -m venv "$venv"; fi
fi
paquets=('tiktoken==0.14.0')
if [[ "$headroom" == 1 ]]; then paquets+=('headroom-ai[code]==0.39.1' 'onnxruntime==1.24.4'); fi
if command -v uv >/dev/null 2>&1; then
  uv pip install --quiet --python "$venv/bin/python" "${paquets[@]}"
else
  "$venv/bin/python" -m pip install --quiet "${paquets[@]}"
fi

echo "== 2/5 oracle construit depuis l'archive épinglée"
recu="$repo_dir/target/rtk-parity/oracle-build/receipt.json"
if [[ ! -x "$repo_dir/target/rtk-parity/oracle-build/release/rtk" || ! -f "$recu" ]]; then
  "$venv/bin/python" -I "$repo_dir/bench/real/build_oracle.py" >"$qa_dir/oracle.log" 2>&1 \
    || { tail -20 "$qa_dir/oracle.log" >&2; echo "construction de l'oracle échouée (journal : $qa_dir/oracle.log)" >&2; exit 1; }
fi
oracle_rtk="$repo_dir/target/rtk-parity/oracle-build/release/rtk"

echo "== 3/5 binaire lm-resizer"
if [[ -n "${LM_RESIZER_BIN:-}" ]]; then
  lm_bin="$LM_RESIZER_BIN"
else
  cargo build --locked --release -p lm-resizer >"$qa_dir/build.log" 2>&1 \
    || { tail -20 "$qa_dir/build.log" >&2; echo "compilation échouée (journal : $qa_dir/build.log)" >&2; exit 1; }
  lm_bin="${CARGO_TARGET_DIR:-$repo_dir/target}/release/lm-resizer"
fi
[[ -x "$lm_bin" ]] || { echo "binaire introuvable : $lm_bin" >&2; exit 1; }
echo "   $lm_bin ($(git rev-parse --short HEAD 2>/dev/null || echo 'sans git'))"

echo "== 4/5 rejeu des 61 captures dans $work"
args=(--binary "$lm_bin" --rtk "$oracle_rtk" --built-oracle "$recu" --work "$work" --repetitions "$repetitions")
if [[ "$headroom" == 1 ]]; then args+=(--headroom-python "$venv/bin/python"); fi
mkdir -p "$(dirname "$work")"
statut=0
"$venv/bin/python" "$repo_dir/bench/real/parity_rtk.py" "${args[@]}" >"$qa_dir/dernier-rejeu.log" 2>&1 || statut=$?
tail -3 "$qa_dir/dernier-rejeu.log" | cut -c1-200
[[ -f "$work/results.json" ]] || { echo "pas de results.json (code $statut) : voir $qa_dir/dernier-rejeu.log" >&2; exit 1; }

echo "== 5/5 synthèse"
# Les seuils par défaut sont ceux de la 0.2.6 (61 captures, médiane 4,87 %, moyenne 28,39 %) ; ceux de la 0.2.5 (25,18 % et
# 34,68 %) supposaient des vues `git log` qui cachaient tous les commits sauf le premier. La position de la
# médiane par rapport à celle de l'oracle est affichée, plus exigée (voir synthese_rejeu.py).
"$venv/bin/python" -I "$repo_dir/bench/real/synthese_rejeu.py" "$work/results.json" ${seuils[@]+"${seuils[@]}"}
