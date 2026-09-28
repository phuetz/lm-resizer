#!/usr/bin/env python3
"""Replay the fixed corpus through all products and calculate qualified savings."""
import argparse
import html
import hashlib
import json
import os
import re
import statistics
import subprocess
import sys
import time
from collections import defaultdict
from pathlib import Path

import tiktoken

ROOT = Path(__file__).resolve().parent
WORKTREE = ROOT.parent
QA = WORKTREE / "_qa" / "banc"
ENC = tiktoken.get_encoding("o200k_base")
TOOLS = ("LM Resizer", "RTK", "Headroom")


def tokens(s):
    return len(ENC.encode(s))


def median(values):
    return statistics.median(values) if values else 0


def setup_shims():
    directory = QA / "shims"
    directory.mkdir(parents=True, exist_ok=True)
    replay = directory / "replay"
    replay.write_text('#!/bin/sh\ncat "$BANC_FIXTURE"\nexit "$BANC_EXIT"\n')
    replay.chmod(0o755)
    for name in ("cargo", "dotnet", "npm", "pytest", "git", "docker", "psql", "journalctl"):
        link = directory / name
        if link.is_symlink() or link.exists():
            link.unlink()
        link.symlink_to(replay.name)
    return directory


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--report", type=Path, default=QA / "RAPPORT.md")
    parser.add_argument("--output", type=Path, default=QA / "results")
    parser.add_argument("--lm-bin", type=Path, default=QA / "lm-target/release/lm-resizer")
    parser.add_argument("--report-only", action="store_true")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    args.report.parent.mkdir(parents=True, exist_ok=True)
    cases = json.loads((ROOT / "cases.json").read_text())
    if args.report_only:
        rows = json.loads((ROOT / "resultats.json").read_text())
        args.report.write_text(render_report(rows, cases, args))
        return
    rtk = QA / "rtk/rtk"
    python = QA / "venv/bin/python"
    shims = setup_shims()
    base_env = os.environ.copy()
    base_env.update(PATH=f"{shims}:{base_env['PATH']}",
                    XDG_CACHE_HOME=str(QA / "cache"),
                    XDG_DATA_HOME=str(QA / "data"),
                    XDG_CONFIG_HOME=str(QA / "config"),
                    HF_HOME=str(QA / "hf"),
                    BANC_EXIT="0")
    rows = []
    for case in cases:
        raw = (ROOT / case["file"]).read_text()
        before = tokens(raw)
        fixture = ROOT / case["file"]
        command = case.get("command") or "cat"
        argv = command.split()
        is_command = argv[0] != "cat"
        env = base_env.copy()
        env["BANC_FIXTURE"] = str(fixture)
        env["BANC_EXIT"] = "1" if (case["id"].endswith("fail") or case["id"] in ("docker", "psql", "compile_error")) else "0"
        for tool in TOOLS:
            home = QA / ("home-" + tool.lower().replace(" ", "-"))
            home.mkdir(exist_ok=True)
            tool_env = env.copy()
            tool_env["HOME"] = str(home)
            if tool == "LM Resizer":
                if is_command:
                    cmd = [str(args.lm_bin), "exec", "--store", str(QA / "lm-ccr.sqlite"), "--", *argv]
                    route = "exec"
                else:
                    cmd = [str(args.lm_bin), "compress", "--store", str(QA / "lm-ccr.sqlite")]
                    route = "compress"
            elif tool == "RTK":
                if is_command:
                    cmd = [str(rtk), *argv]
                    route = "commande native"
                else:
                    cmd = [str(rtk), "pipe"]
                    route = "pipe"
                tool_env["RTK_NO_TELEMETRY"] = "1"
            else:
                cmd = [str(python), str(ROOT / "headroom_once.py")]
                route = "API Python"
            start = time.perf_counter_ns()
            try:
                proc = subprocess.run(cmd, input=None if is_command and tool != "Headroom" else raw,
                                      text=True, capture_output=True, timeout=45, env=tool_env,
                                      cwd=QA)
                elapsed_ms = (time.perf_counter_ns() - start) / 1e6
                out = proc.stdout
                error = None
                if proc.returncode not in (0, int(env["BANC_EXIT"])):
                    error = f"code de sortie inattendu {proc.returncode}"
                if not out.strip():
                    error = error or "sortie vide"
            except (subprocess.TimeoutExpired, OSError) as exc:
                elapsed_ms = (time.perf_counter_ns() - start) / 1e6
                out = ""
                error = type(exc).__name__
                proc = None
            after = tokens(out) if out else 0
            missing = check_oracle(case, out)
            retention = (len(case["oracle"]) - len(missing)) / len(case["oracle"]) if not error else 0
            saving = 1 - after / before if before else 0
            qualified = saving if retention == 1 and not error else 0
            result = dict(case=case["id"], category=case["category"], tool=tool,
                          route=route, tokens_before=before, tokens_after=after,
                          saving=saving, qualified_saving=qualified, oracle_retention=retention,
                          missing=missing, latency_ms=elapsed_ms, error=error,
                          exit_code=proc.returncode if proc else None,
                          sha256=hashlib.sha256(out.encode()).hexdigest())
            rows.append(result)
            (args.output / f"{case['id']}-{tool.lower().replace(' ', '-')}.txt").write_text(out)
            print(f"{case['id']:17} {tool:11} {before:5}->{after:5} oracle={retention:.0%} {elapsed_ms:.0f}ms {error or ''}", flush=True)
    (args.output / "results.json").write_text(json.dumps(rows, indent=2, ensure_ascii=False) + "\n")
    (ROOT / "resultats.json").write_text(json.dumps(rows, indent=2, ensure_ascii=False) + "\n")
    report = render_report(rows, cases, args)
    args.report.write_text(report)


def check_oracle(case, output):
    if case["id"] != "json_large":
        return [fact for fact in case["oracle"] if fact not in output]
    # SmartCrusher may turn an array of objects into a compact row table.
    # Validate the fields together in the anomalous row, not isolated digits.
    try:
        obj = json.loads(output)
        if obj.get("schema") != "orders-v3" or obj.get("count") != 180:
            return case["oracle"]
        records = obj.get("rows", [])
        if isinstance(records, list):
            row = next((r for r in records if r.get("id") == 143), None)
            if row and row.get("amount") == 4299 and row.get("state") == "rejected" and row.get("meta", {}).get("reason") == "limit_exceeded":
                return []
        if isinstance(records, str):
            line = next((line for line in records.splitlines() if "4299,143" in line), "")
            if "rejected" in line and "limit_exceeded" in line:
                return []
    except (ValueError, TypeError, AttributeError):
        pass
    return case["oracle"]


def render_report(rows, cases, args):
    bench_commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=WORKTREE, text=True).strip()
    default_binary = QA / "lm-target/release/lm-resizer"
    binary_hash = hashlib.sha256(args.lm_bin.read_bytes()).hexdigest()
    binary_origin = (f"construit depuis ce checkout (`{bench_commit}`)" if args.lm_bin.resolve() == default_binary.resolve()
                     else "fourni par `LM_RESIZER_BIN` ; révision source à documenter séparément")
    head = [
        "# Banc comparatif LM Resizer / RTK / Headroom",
        "",
        "Résultat sur le commit testé : **aucune preuve de supériorité globale** n'est déclarée automatiquement. Une économie est qualifiée seulement si 100 % des faits de l'oracle restent visibles. Les originaux récupérables ne remplacent pas cette condition.",
        "",
        "## Versions et protocole",
        "",
        f"- LM Resizer : binaire {binary_origin} ; SHA-256 `{binary_hash}`.",
        "- RTK : `0.50.0`, release officielle, archive Linux musl vérifiée par SHA-256.",
        "- Headroom : paquet PyPI `headroom-ai==0.39.1` avec extra `code`.",
        "- Tokenizer commun : `tiktoken` `o200k_base` ; économies de sortie, pas de facturation fournisseur.",
        "- Chaque cas est rejoué sur les mêmes octets. LM Resizer utilise `exec` pour les commandes et `compress` pour les fichiers ; RTK utilise ses commandes natives et `pipe` pour les fichiers ; Headroom reçoit un résultat d'outil via `compress(messages)`.",
        "- Les temps couvrent un processus complet par cas, y compris le démarrage Python de Headroom. Il faut une mesure en processus persistant pour comparer le débit serveur.",
        "- RTK et LM Resizer rejouent une commande factice isolée qui émet la fixture et son code de sortie. Aucun outil réel ni service actif n'est invoqué par les cas.",
        "- Oracle : correspondance littérale des faits déclarés dans `cases.json`, sauf JSON validé par structure ou ligne tabulaire équivalente. Aucune récupération CCR. La valeur médiane qualifiée est zéro quand un fait manque ; une croissance reste négative.",
        "- Les 66 mesures détaillées (`tokens_before`, `tokens_after`, rétention, temps, code de sortie et faits manquants) sont versionnées dans `bench/resultats.json` ; les sorties complètes restent dans `_qa/banc/results/`.",
        "",
        "## Résultats par catégorie",
        "",
        "| Catégorie | Cas | LM Resizer : médiane / oracle | RTK : médiane / oracle | Headroom : médiane / oracle |",
        "|---|---:|---:|---:|---:|",
    ]
    try:
        stats_env = os.environ.copy()
        stats_env["HOME"] = str(QA / "home-lm-resizer")
        stats = subprocess.check_output([str(args.lm_bin), "stats", "--markdown", "--store", str(QA / "lm-ccr.sqlite")], text=True, timeout=10, env=stats_env)
        commands = re.search(r"Exec commands: (\d+)", stats)
        bytes_saved = re.search(r"Bytes saved: (\d+)", stats)
        estimated = re.search(r"Estimated tokens saved: (\d+)", stats)
        if commands and bytes_saved and estimated:
            head.insert(head.index("## Résultats par catégorie") - 1,
                        f"- Statistiques LM Resizer cumulées du stockage local : {commands.group(1)} commandes enveloppées, {bytes_saved.group(1)} octets et {estimated.group(1)} jetons estimés économisés ; elles couvrent tous les rejeux locaux et ne sont pas utilisées dans les tableaux.")
    except (OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired):
        pass
    categories = sorted({c["category"] for c in cases})
    for category in categories + ["GLOBAL"]:
        subset = [r for r in rows if category == "GLOBAL" or r["category"] == category]
        n = len(subset) // 3
        cells = []
        for tool in TOOLS:
            group = [r for r in subset if r["tool"] == tool]
            q = median([r["qualified_saving"] for r in group]) * 100
            o = sum(r["oracle_retention"] for r in group) / len(group) * 100
            cells.append(f"{q:.1f} % / {o:.1f} %")
        head.append(f"| {category} | {n} | " + " | ".join(cells) + " |")
    head += ["", "## Latence et échecs techniques", "",
             "| Outil | Médiane par cas | Maximum | Échecs techniques |",
             "|---|---:|---:|---:|"]
    for tool in TOOLS:
        group = [r for r in rows if r["tool"] == tool]
        head.append(f"| {tool} | {median([r['latency_ms'] for r in group]):.0f} ms | {max(r['latency_ms'] for r in group):.0f} ms | {sum(bool(r['error']) for r in group)} |")
    head += ["", "## Gagnants et pertes par cas", "", "| Cas | Gagnant qualifié | LM Resizer | RTK | Headroom |", "|---|---|---:|---:|---:|"]
    for case in cases:
        trio = [next(r for r in rows if r["case"] == case["id"] and r["tool"] == tool) for tool in TOOLS]
        eligible = [r for r in trio if r["oracle_retention"] == 1 and not r["error"]]
        best = max((r["qualified_saving"] for r in eligible), default=0)
        winners = [r["tool"] for r in eligible if r["qualified_saving"] == best]
        winner = ", ".join(winners) if winners else "aucun"
        cells = [f"{r['saving']*100:.0f} % / {r['oracle_retention']*100:.0f} %" + (" ⚠" if r["error"] else "") for r in trio]
        head.append(f"| {case['id']} | {winner} | " + " | ".join(cells) + " |")
    head += ["", "## Faits perdus et échecs", ""]
    bad = [r for r in rows if r["missing"] or r["error"]]
    if bad:
        for r in bad:
            facts = ", ".join(f"<code>{html.escape(x)}</code>" for x in r["missing"])
            head.append(f"- `{r['case']}` / {r['tool']} : {r['error'] or 'oracle incomplet'} ; manquent {facts or 'aucun fait'}.")
    else:
        head.append("Aucun fait manquant ni échec technique.")
    head += ["", "## Ce que LM Resizer doit améliorer pour surpasser les deux", ""]
    for case in cases:
        trio = {tool: next(r for r in rows if r["case"] == case["id"] and r["tool"] == tool) for tool in TOOLS}
        lm = trio["LM Resizer"]
        best_other = max((trio[tool] for tool in ("RTK", "Headroom")), key=lambda r: r["qualified_saving"])
        if lm["oracle_retention"] < 1:
            head.append(f"- `{case['id']}` : conserver les faits perdus par LM Resizer ({', '.join(html.escape(x) for x in lm['missing'])}).")
        elif lm["qualified_saving"] < best_other["qualified_saving"]:
            head.append(f"- `{case['id']}` : dépasser {best_other['tool']} ({best_other['qualified_saving']*100:.1f} % d'économie qualifiée) en gardant l'oracle complet ; LM Resizer atteint {lm['qualified_saving']*100:.1f} %.")
        elif lm["saving"] < 0:
            head.append(f"- `{case['id']}` : supprimer la croissance de sortie ({lm['saving']*100:.1f} %) sans perdre les faits obligatoires.")
    tied_zero = [c["id"] for c in cases if all(next(r for r in rows if r["case"] == c["id"] and r["tool"] == tool)["qualified_saving"] == 0 for tool in TOOLS)]
    if tied_zero:
        head.append("- Sortir de l'égalité à zéro sur " + ", ".join(f"`{case_id}`" for case_id in tied_zero) + " avec une économie strictement positive et un oracle complet.")
    head += ["- Surpasser les deux en médiane globale et dans chaque catégorie, sur une branche intégrée et avec les mêmes oracles ; confirmer la capacité de réparation par agent avant une publication large.",
             "", "## Sources officielles", "",
             "- [RTK, release v0.50.0](https://github.com/rtk-ai/rtk/releases/tag/v0.50.0) et [mode pipe](https://github.com/rtk-ai/rtk/blob/develop/README.md).",
             "- [Headroom, installation et API Python](https://github.com/headroomlabs-ai/headroom/blob/main/README.md).",
             "- [Comparaison documentaire du 22/09](../../20260922-lm-resizer-comparaison-actualisee/COMPARAISON.md) (lecture préalable, sans résultat de banc).",
             ""]
    agent_path = ROOT / "resultats_agent.json"
    if agent_path.exists():
        head += ["## Tâche d'agent de bout en bout", "",
                 "Trois essais Codex isolés reçoivent chacun la sortie compressée de `pytest_fail`, lisent le petit projet de test et tentent de corriger le code. Le test final est relancé indépendamment après l'agent.",
                 "", "| Sortie reçue | Agent | Test final | Code modifié |", "|---|---:|---:|---:|"]
        for result in json.loads(agent_path.read_text()):
            outcome = "non évalué (sandbox)" if result.get("environment_blocked") else ("réussi" if result["tests_pass"] else "échec")
            head.append(f"| {result['tool']} | {result['agent_exit']} | {outcome} | {'oui' if result['source_changed'] else 'non'} |")
        head += ["", "Les essais bloqués par le sandbox ne mesurent aucun compresseur. Si les essais s'exécutent, ce test mesure la réparation avec les fichiers du projet accessibles ; il ne prouve pas à lui seul que la sortie compressée suffisait.", ""]
    head += ["## Ce que je n'ai pas pu vérifier", "",
             "- Coûts facturés et cache fournisseur : aucune requête à un modèle payant.",
             "- Équivalence en production des intégrations proxy, hooks et CCR entre les trois outils.",
             "- Tokenizer Claude officiel : indisponible localement ; `o200k_base` est appliqué identiquement.",
             "- Réparation par agent : le Codex imbriqué n'a pas pu ouvrir son verrou de sandbox en écriture ; aucun essai n'a atteint la modification du code.",
             "- Généralisation statistique : les 22 cas sont déterministes et synthétiques, sans données personnelles.", ""]
    return "\n".join(head)


if __name__ == "__main__":
    main()
