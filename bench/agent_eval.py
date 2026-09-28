#!/usr/bin/env python3
"""Run three isolated Codex repair attempts from the captured compressed output."""
import json
import os
import shutil
import signal
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
QA = ROOT.parent / "_qa/banc"
RESULTS = QA / "results"


def evaluate(tool):
    slug = tool.lower().replace(" ", "-")
    work = QA / "agent" / slug
    if work.exists():
        shutil.rmtree(work)
    shutil.copytree(ROOT / "agent_fixture", work)
    output = (RESULTS / f"pytest_fail-{slug}.txt").read_text()
    prompt = f"""Tu es un agent de réparation. Voici la sortie compressée du test en échec, seule sortie de test fournie au départ :

{output}

Dans ce dossier isolé, corrige le code de production pour que tous les tests passent. Tu peux lire le code et les tests et lancer `{QA}/venv/bin/python -m pytest -q`. Ne consulte aucun fichier hors de ce dossier, à part l'interpréteur Python indiqué. Ne récupère pas la sortie brute depuis RTK/CCR. Explique brièvement le correctif.\n"""
    cmd = ["codex", "exec", "--ephemeral", "--skip-git-repo-check", "--ignore-rules",
           "--sandbox", "workspace-write", "-C", str(work), "-"]
    env = os.environ.copy()
    codex_home = QA / "codex-home"
    codex_home.mkdir(exist_ok=True)
    auth = codex_home / "auth.json"
    if not auth.exists():
        auth.symlink_to(Path.home() / ".codex/auth.json")
    env["CODEX_HOME"] = str(codex_home)
    log = QA / "agent" / f"{slug}.log"
    with log.open("w") as stream:
        process = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=stream, stderr=subprocess.STDOUT,
                                   text=True, start_new_session=True, env=env)
        try:
            process.communicate(prompt, timeout=180)
            agent_exit = process.returncode
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.communicate()
            agent_exit = -9
    check = subprocess.run([str(QA / "venv/bin/python"), "-m", "pytest", "-q"],
                           cwd=work, text=True, capture_output=True, timeout=20)
    environment_blocked = "sandbox" in log.read_text().lower() and "Read-only file system" in log.read_text()
    return {"tool": tool, "agent_exit": agent_exit, "environment_blocked": environment_blocked,
            "tests_pass": check.returncode == 0,
            "test_output": check.stdout[-600:], "log": str(log.relative_to(QA)),
            "source_changed": (work / "orders.py").read_bytes() != (ROOT / "agent_fixture/orders.py").read_bytes()}


def main():
    out = []
    for tool in ("LM Resizer", "RTK", "Headroom"):
        result = evaluate(tool)
        out.append(result)
        label = "NON ÉVALUÉ (sandbox)" if result["environment_blocked"] else ("OK" if result["tests_pass"] else "ÉCHEC")
        print(f"{tool}: agent={result['agent_exit']} test={label}", flush=True)
    (ROOT / "resultats_agent.json").write_text(json.dumps(out, indent=2, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    main()
