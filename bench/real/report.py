#!/usr/bin/env python3
"""Render honest unweighted case statistics; never zero-out an oracle failure."""
import argparse
import json
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--before', type=Path, required=True)
parser.add_argument('--after', type=Path, required=True)
parser.add_argument('--rtk', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
before, after, rtk = [json.loads(p.read_text()) for p in (args.before, args.after, args.rtk)]
old = {c['id']: c for c in before['cases']}
other = {c['id']: c for c in rtk['cases']}
lines = ['# Banc réel : avant / après / RTK', '',
         '30 cas principaux, trois dépôts publics épinglés; cinq cas de comparaison supplémentaires.',
         'tiktoken 0.14.0; comptes réels des sorties, marqueurs inclus; médianes et moyennes non pondérées.',
         'Une économie reste comptée même si des faits disparaissent. Aucun résultat « qualifié » remis à zéro.', '',
         '| Encodage | Mesure | LMR avant | LMR après | RTK 0.50.0 |', '|---|---|---:|---:|---:|']
for enc in ('cl100k_base', 'o200k_base'):
    for key, title in [('median','médiane'),('mean','moyenne')]:
        values = [doc['summary'][enc][key] for doc in (before, after, rtk)]
        lines.append('| '+enc+' | '+title+' | '+' | '.join(f'{v:.2f} %' for v in values)+' |')
provenance = after.get('source_provenance') or {}
lines += ['', 'Sources LMR mesurées : `'+provenance.get('source_commit', 'historique sans manifeste')+'`.',
          'SHA-256 du binaire LMR après : `'+after['binary_sha256']+'`.',
          'Les commits documentaires ultérieurs ne changent pas cette provenance. Les sources et la commande de compilation sont dans le JSON.', '']
facts = sum(c['facts'] for c in after['cases'])
lost = sum(c['missing_count'] for c in after['cases'])
old_lost = sum(c['missing_count'] for c in before['cases'])
recovered = sum(c.get('recovery_verified') is True for c in after['cases'])
lines += ['', f'Faits LMR conservés : **{facts-lost:,}/{facts:,}** après; {old_lost} faits non reconnus avant. '
          f'{recovered}/{sum(c.get("recovery_verified") is not None for c in after["cases"])} cas avec restitution tee vérifiés à l’octet près; les autres cas ne créent pas de tee. '
          f'Codes de sortie conservés : {sum(c["exit_preserved"] for c in after["cases"])}/{len(after["cases"])}.', '',
          '## Détail o200k (les deux encodages sont dans les JSON)', '',
          '| Cas | Brut | LMR avant | LMR après | RTK | LMR avant s | LMR après s | Faits absents LMR après |',
          '|---|---:|---:|---:|---:|---:|---:|---:|']
for case in after['cases']:
    key = case['id']; b = old[key]; r = other[key]
    lines.append(f'| {key} | {case["tokens"]["o200k_base"]["raw"]:,} | {b["tokens"]["o200k_base"]["output"]:,} | '
                 f'{case["tokens"]["o200k_base"]["output"]:,} | {r["tokens"]["o200k_base"]["output"]:,} | '
                 f'{b["exec_seconds"]:.3f} | {case["exec_seconds"]:.3f} | {case["missing_count"]} |')
lines += ['', '## Interprétation et limites', '',
          '- Les temps LMR incluent le CLI, un enfant de rejeu, la compression, les comptes exacts, tee et SQLite. Une seule passe par cas.',
          '- RTK utilise une capture identique via `pipe` lorsque son filtre existe; les autres cas sont relancés. Les modes et temps séparés sont dans `rtk.json`.',
          '- Le filtre RTK pipe ne propage pas le code de la commande initiale. On ne compare pas ce code à celui du rejeu LMR.',
          '- L’oracle décode les regroupements grep/find RTK; des reformulations restent non reconnues. Son compteur n’est pas une preuve universelle de pertes sémantiques.',
          '- Les lignes grep et les chemins sont vérifiés avec leur multiplicité; les fichiers cat à l’octet UTF-8 près, espaces et fin de ligne inclus.',
          '- Les réussites individuelles peuvent être résumées; chaque diagnostic et chaque compteur de résultat restent exigés.',
          '- Les captures de test ne sont pas des certifications des projets : dépendances Python incomplètes, erreurs réelles conservées, npm sans hereby.',
          '- Le rapport Grok initial ne donnait pas les commits : ce banc fixe ses propres entrées, pas une reproduction au jeton près de cette première mesure.',
          '- Aucune mesure de facture fournisseur, qualité des décisions des agents ou performance universelle.', '']
args.output.write_text('\n'.join(lines))
