#!/usr/bin/env sh
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$root"

fail=0

for phrase in "after the v0.2.3 release" "Once the v0.2.3 release" "Build from source now" "après la publication de v0.2.3" "Une fois la release v0.2.3"; do
    if grep -qF "$phrase" README.md README.fr.md 2>/dev/null; then
        printf "Erreur : la phrase '%s' a été trouvée dans README.md ou README.fr.md.\n" "$phrase" >&2
        fail=1
    fi
done

version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)

for file in README.md README.fr.md; do
    if ! grep -qF "v$version/install.sh" "$file"; then
        printf "Erreur : commande install.sh dans %s ne mentionne pas la version v%s.\n" "$file" "$version" >&2
        fail=1
    fi
    if ! grep -qF "v$version/install.ps1" "$file"; then
        printf "Erreur : commande install.ps1 dans %s ne mentionne pas la version v%s.\n" "$file" "$version" >&2
        fail=1
    fi
done

if [ "$fail" -eq 1 ]; then
    exit 1
fi

printf "Vérification README install : OK\n"
