#!/usr/bin/env python3
"""Verdict du rejeu des 61 captures, lu dans results.json (voir rejouer.sh).

Le verdict est « tenu » seulement si tout ceci est vrai :
- le fichier couvre exactement le nombre de captures attendu (61 par défaut) ;
- la médiane et la moyenne de jetons économisés, rappel du brut compris, atteignent les seuils
  (ceux de la 0.2.6 par défaut : 4,87 % et 27,60 %, voir ci-dessous) ;
- chaque brut est récupérable et chaque code de sortie du producteur est conservé.
L'égalité stricte des vues n'est pas une condition : elle est affichée. La position de la médiane par
rapport à celle de l'oracle de comparaison l'est aussi, sans être une condition : depuis que `pipe` et
`tool-output` ne raccourcissent plus un `git log`, la médiane de LM Resizer (4,87 %) est sous celle de
l'oracle (15,81 %), et le verdict ne doit pas exiger le contraire.
"""
import argparse
import json
import sys

CAPTURES = 61
# Seuils de la 0.2.6 (rejeu du 9 octobre 2026). Ceux de la 0.2.5 (25,18 % et 34,68 %) comptaient cinq
# captures `git log` à 91-97 % obtenues en ne montrant que le premier commit. Depuis que la compression de
# `git log` n'existe que pour `exec` au format par défaut prouvé, ces captures (passées par `pipe`) sont
# rendues brutes : la médiane retombe à 4,87 % (elle valait 21,32 %, valeur d'une de ces captures) et la
# moyenne à 28,39 %. Rejeu du 9 octobre 2026 (soir) : `npm test` lance un script choisi par l'utilisateur
# et sort brut (la vue `packages` supprimait les lignes vides, dont celle d'un commit sans message en
# `git log --format=%s`) ; seules TypeScript-test (14 → 28 jetons sur 31) et visible-jest (607 → 624 sur 627)
# changent, la moyenne passe à 27,61 %. Les seuils sont ces mesures, arrondies par défaut : un plancher
# contre la régression.
MEDIANE_MIN = 4.87
MOYENNE_MIN = 27.60


def verdict(results, captures=CAPTURES, mediane_min=MEDIANE_MIN, moyenne_min=MOYENNE_MIN):
    """Retourne (lignes à afficher, liste des raisons d'échec)."""
    rows, summary = results["cases"], results["summary"]
    n = len(rows)
    lm, ref = summary["lm_total_tokens"], summary["rtk_tokens"]
    strict = sum(1 for x in rows if x["parity"])
    brut = sum(1 for x in rows if x["tee_verified"])
    code = sum(1 for x in rows if x["producer_exit_preserved"])
    lines = [
        f"binaire lm-resizer sha256 {results['lm_sha256'][:16]}…  corpus sha256 {results['corpus_sha256'][:16]}…",
        f"LM Resizer, tee compris : médiane {lm['median']:.2f} %  moyenne {lm['mean']:.2f} %  ({lm['cases']} captures)",
        f"oracle                  : médiane {ref['median']:.2f} %  moyenne {ref['mean']:.2f} %",
    ]
    headroom = summary.get("headroom_tokens") or {}
    if headroom.get("median") is not None:
        lines.append(f"Headroom                : médiane {headroom['median']:.2f} %  moyenne {headroom['mean']:.2f} %")
    lines.append(
        f"vues strictement égales {strict}/{n} ; bruts récupérés {brut}/{n} ; codes du producteur conservés {code}/{n}"
    )
    lines.append(f"seuils exigés           : {captures} captures, médiane ≥ {mediane_min:.2f} %, moyenne ≥ {moyenne_min:.2f} %")
    if lm["median"] is not None:
        position = "au-dessus de" if lm["median"] >= ref["median"] else "sous"
        lines.append(f"médiane de LM Resizer {position} celle de l'oracle (information, pas une condition)")
    failures = []
    if n != captures or lm["cases"] != captures:
        failures.append(f"{n} captures dans le fichier ({lm['cases']} mesurées), {captures} attendues")
    if lm["median"] is None or lm["median"] < mediane_min:
        failures.append(f"médiane {lm['median']} % sous le seuil {mediane_min} %")
    if lm["mean"] is None or lm["mean"] < moyenne_min:
        failures.append(f"moyenne {lm['mean']} % sous le seuil {moyenne_min} %")
    if brut != n:
        failures.append(f"{n - brut} brut(s) non récupérable(s)")
    if code != n:
        failures.append(f"{n - code} code(s) de sortie du producteur perdu(s)")
    return lines, failures


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("results", help="results.json écrit par parity_rtk.py")
    parser.add_argument("--captures", type=int, default=CAPTURES)
    parser.add_argument("--mediane-min", type=float, default=MEDIANE_MIN)
    parser.add_argument("--moyenne-min", type=float, default=MOYENNE_MIN)
    args = parser.parse_args()
    with open(args.results, encoding="utf-8") as handle:
        results = json.load(handle)
    lines, failures = verdict(results, args.captures, args.mediane_min, args.moyenne_min)
    print("\n".join(lines))
    if failures:
        print("VERDICT : NON tenu — " + " ; ".join(failures))
        sys.exit(1)
    print("VERDICT : tenu")


if __name__ == "__main__":
    main()
