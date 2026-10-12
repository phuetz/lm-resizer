#!/usr/bin/env python3
"""Mesure trois sources réelles sans les exécuter ; jetons approchés = octets / 4."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time


def main():
    sys.stdout.reconfigure(encoding='utf-8')
    root = Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    records = []
    for relative in ['src/outline_view.rs', 'packages/wasm/index.js', 'bench/real/build.py']:
        source = (root / relative).read_bytes()
        started = time.perf_counter()
        result = subprocess.run(
            [args.binary, 'smart', relative, '--ast', '--json'],
            cwd=root, capture_output=True, check=True,
        )
        elapsed_ms = (time.perf_counter() - started) * 1000
        assert not result.stderr, result.stderr.decode('utf-8', errors='replace')
        report = json.loads(result.stdout)
        assert report['steps_applied'] == ['smart_ast'], report
        summary = report['output']
        before, after = len(source), len(summary.encode('utf-8'))
        assert (root / relative).read_bytes() == source, 'source modifiée pendant la mesure'
        assert report['original_bytes'] == before
        assert report['compressed_bytes'] == after
        assert report['original_tokens'] == before / 4
        assert report['compressed_tokens'] == after / 4
        assert after < before, 'résumé pas plus petit pour ce cas réel'
        records.append({
            'file': relative,
            'input_sha256': hashlib.sha256(source).hexdigest(),
            'input_bytes': before,
            'output_bytes': after,
            'input_approx_tokens': before / 4,
            'output_approx_tokens': after / 4,
            'saved_percent': round((before - after) / before * 100, 2),
            'elapsed_ms': round(elapsed_ms, 3),
            'output': summary,
        })
        print(f'{relative}: {before} → {after} octets ; {before / 4} → {after / 4} jetons approchés ; {elapsed_ms:.3f} ms')
        print(summary)
    evidence = {
        'token_estimate': 'Approximation : octets UTF-8 divisés par 4 ; aucune tokenisation.',
        'timing': 'Une exécution par fichier, durée du processus CLI incluse ; pas une garantie de latence.',
        'files': records,
    }
    (root / args.output).write_text(json.dumps(evidence, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')


if __name__ == '__main__':
    main()
