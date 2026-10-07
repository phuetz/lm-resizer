#!/usr/bin/env bash
# Échoue si une commande, sous-commande, option longue ou variable d'environnement
# affichée par `lm-resizer --help` (récursivement) manque dans docs/CLI-REFERENCE.md.
set -uo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin="${1:-${LM_RESIZER_BIN:-$root/target/release/lm-resizer}}"
ref="$root/docs/CLI-REFERENCE.md"
[ -x "$bin" ] || { echo "binaire introuvable : $bin" >&2; exit 2; }
[ -f "$ref" ] || { echo "référence introuvable : $ref" >&2; exit 2; }
home="$(mktemp -d)"; trap 'rm -rf "$home"' EXIT
help() { HOME="$home" "$bin" "$@" --help 2>&1; }
subs() { # sous-commandes listées dans la section Commands: (hors help)
  awk '/^Commands:/{on=1;next} /^[A-Za-z]/{on=0} on && /^  [a-z]/{print $1}' | grep -vx help
}
missing=0; checked=0
need() { # $1 = type, $2 = motif exact, $3 = étiquette
  checked=$((checked+1))
  if ! grep -Eq -- "$2" "$ref"; then echo "MANQUE ($1) : $3"; missing=$((missing+1)); fi
}
need_option_for() { # l'option doit figurer sur une ligne qui cite aussi la commande
  local o="$1" c="$2" last="${2##* }"
  checked=$((checked+1))
  if ! grep -E -- "(^|[^a-z0-9-])$o([^a-z0-9-]|$)" "$ref" | grep -Eq -- "(^|[^a-z0-9-])(${c// /[ ]}|$last)([^a-z0-9-]|$)"; then
    echo "MANQUE (option) : $o  (aide de : $c, sur une ligne qui cite la commande)"; missing=$((missing+1))
  fi
}
cmds=()
while read -r c; do cmds+=("$c"); done < <(help | subs)
all=("${cmds[@]}")
for c in "${cmds[@]}"; do
  while read -r s; do [ -n "$s" ] && all+=("$c $s"); done < <(help "$c" | subs)
done
for c in "${all[@]}"; do
  need commande "\`$c([ \`]|\`)" "$c"
  # shellcheck disable=SC2086
  text="$(help $c)"
  while read -r o; do
    need_option_for "$o" "$c"
  done < <(printf '%s\n' "$text" | grep -oE '^ +(-[A-Za-z], )?--[a-z0-9][a-z0-9-]*' | grep -oE -- '--[a-z0-9-]+' | grep -vx -e '--help')
  while read -r v; do
    need variable "$v" "$v  (aide de : $c)"
  done < <(printf '%s\n' "$text" | grep -oE '\[env: [A-Z_]+' | sed 's/\[env: //')
done
echo "check-cli-reference : $checked éléments vérifiés, $missing manquant(s)"
[ "$missing" -eq 0 ]
