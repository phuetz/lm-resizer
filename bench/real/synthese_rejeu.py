#!/usr/bin/env python3
"""Verdict du rejeu des 61 captures, lu dans results.json (voir rejouer.sh).

Le verdict est « tenu » seulement si tout ceci est vrai :
- le fichier couvre exactement le nombre de captures attendu (61 par défaut) ;
- la médiane et la moyenne de jetons économisés, rappel du brut compris, atteignent les seuils
  (ceux publiés pour la 0.2.5 par défaut : 25,18 % et 34,68 %) ;
- la médiane atteint celle de l'oracle de comparaison ;
- chaque brut est récupérable et chaque code de sortie du producteur est conservé.
L'égalité stricte des vues n'est pas une condition : elle est affichée.
"""
import argparse
import json
import sys

CAPTURES = 61
MEDIANE_MIN = 25.18
MOYENNE_MIN = 34.68


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
    failures = []
    if n != captures or lm["cases"] != captures:
        failures.append(f"{n} captures dans le fichier ({lm['cases']} mesurées), {captures} attendues")
    if lm["median"] is None or lm["median"] < mediane_min:
        failures.append(f"médiane {lm['median']} % sous le seuil {mediane_min} %")
    if lm["mean"] is None or lm["mean"] < moyenne_min:
        failures.append(f"moyenne {lm['mean']} % sous le seuil {moyenne_min} %")
    if not results.get("median_goal_met") or lm["median"] < ref["median"]:
        failures.append("médiane sous celle de l'oracle")
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
