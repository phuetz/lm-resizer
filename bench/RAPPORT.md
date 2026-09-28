# Banc comparatif LM Resizer / RTK / Headroom

Sur ces 22 fixtures, LM Resizer est devant ou à égalité sur chaque économie qualifiée, avec tous ses oracles complets. La généralisation hors de ce corpus n'est pas démontrée. Une économie ne compte que si l'oracle est intégralement conservé.

## Versions et méthode

- Checkout `a6fa800b0b6cabeb04b1a37497a16d6b7f12c8f5` ; SHA-256 du binaire LM Resizer `2f0c63ac6feb67b0e217283760b64c268dd6e0c731a4a3a831b8efaf5e04b0ba`.
- RTK `0.50.0`, Headroom `0.39.1` avec ONNX Runtime `1.24.4`.
- Tokenizer commun : `tiktoken-rs` `o200k_base` ; mêmes octets de fixture pour tous.
- RTK suit la route déclarée dans `cases.json` ; LM Resizer utilise `exec` ou `compress --input` ; Headroom utilise son API `compress(messages)`.
- Les chemins HOME/checkout sont normalisés pour le comptage ; les originaux restent sous `target/banc/results/`.
- Latence : processus complet, démarrage Python de Headroom compris.
- Preuve HOME : 20 contre 20 jetons bruts, 20 contre 20 normalisés, empreintes égales : true.
- Replis Headroom vers la détection Python : 0/22. Détails des 66 mesures : `bench/resultats.json`.

## Résultats par catégorie

| Catégorie | Cas | LM Resizer : médiane / oracle | RTK : médiane / oracle | Headroom : médiane / oracle |
|---|---:|---:|---:|---:|
| build | 1 | 89.0 % / 100.0 % | 0.0 % / 0.0 % | 0.0 % / 100.0 % |
| code | 6 | 0.0 % / 100.0 % | 0.0 % / 100.0 % | 0.0 % / 100.0 % |
| database | 1 | 93.2 % / 100.0 % | 0.0 % / 100.0 % | 0.0 % / 100.0 % |
| git | 2 | 46.7 % / 100.0 % | 0.0 % / 39.6 % | 25.0 % / 100.0 % |
| infra | 1 | 90.0 % / 100.0 % | 0.0 % / 100.0 % | 0.0 % / 100.0 % |
| json | 1 | 65.0 % / 100.0 % | 0.0 % / 33.3 % | 51.9 % / 100.0 % |
| logs | 1 | 96.3 % / 100.0 % | 95.1 % / 100.0 % | 91.6 % / 100.0 % |
| prose | 1 | 84.5 % / 100.0 % | 0.0 % / 100.0 % | 0.0 % / 100.0 % |
| tests | 8 | 92.7 % / 100.0 % | 0.0 % / 41.0 % | 0.0 % / 100.0 % |
| GLOBAL | 22 | 87.4 % / 100.0 % | 0.0 % / 65.5 % | 0.0 % / 100.0 % |

## Latence et échecs

| Outil | Médiane | Maximum | Échecs techniques |
|---|---:|---:|---:|
| LM Resizer | 18 ms | 24 ms | 0 |
| RTK | 10 ms | 40 ms | 0 |
| Headroom | 429 ms | 806 ms | 0 |

## Gagnants et pertes par cas

| Cas | Gagnant qualifié | LM Resizer | RTK | Headroom |
|---|---|---:|---:|---:|
| cargo_ok | LM Resizer | 98 % / 100 % | 97 % / 100 % | 0 % / 100 % |
| cargo_fail | LM Resizer | 87 % / 100 % | 93 % / 50 % | 0 % / 100 % |
| dotnet_ok | LM Resizer | 96 % / 100 % | 97 % / 0 % | 0 % / 100 % |
| dotnet_fail | LM Resizer | 89 % / 100 % | 87 % / 100 % | 0 % / 100 % |
| npm_ok | LM Resizer | 96 % / 100 % | 99 % / 0 % | 0 % / 100 % |
| npm_fail | LM Resizer | 92 % / 100 % | 97 % / 25 % | 0 % / 100 % |
| pytest_ok | LM Resizer | 94 % / 100 % | 99 % / 33 % | 0 % / 100 % |
| pytest_fail | LM Resizer | 87 % / 100 % | 99 % / 20 % | 0 % / 100 % |
| git_diff | LM Resizer | 55 % / 100 % | 74 % / 75 % | 50 % / 100 % |
| git_log | LM Resizer | 38 % / 100 % | 98 % / 4 % | 0 % / 100 % |
| docker | LM Resizer | 90 % / 100 % | 0 % / 100 % | 0 % / 100 % |
| psql | LM Resizer | 93 % / 100 % | 0 % / 100 % | 0 % / 100 % |
| logs | LM Resizer | 96 % / 100 % | 95 % / 100 % | 92 % / 100 % |
| json_large | LM Resizer | 65 % / 100 % | 99 % / 33 % | 52 % / 100 % |
| compile_error | LM Resizer | 89 % / 100 % | 97 % / 0 % | 0 % / 100 % |
| code_csharp | aucun gain qualifié | 0 % / 100 % | 0 % / 100 % | 0 % / 100 % |
| code_rust | aucun gain qualifié | 0 % / 100 % | 0 % / 100 % | 0 % / 100 % |
| code_python | aucun gain qualifié | 0 % / 100 % | 0 % / 100 % | 0 % / 100 % |
| code_typescript | aucun gain qualifié | 0 % / 100 % | 0 % / 100 % | 0 % / 100 % |
| code_go | aucun gain qualifié | 0 % / 100 % | 0 % / 100 % | 0 % / 100 % |
| code_java | aucun gain qualifié | 0 % / 100 % | 0 % / 100 % | 0 % / 100 % |
| prose | LM Resizer | 84 % / 100 % | 0 % / 100 % | 0 % / 100 % |

## Faits manquants par outil

- `cargo_fail` / RTK : oracle incomplet ; manquent src/parser.rs:42:9, expected=422 observed=200.
- `dotnet_ok` / RTK : oracle incomplet ; manquent Test Run Successful, Passed: 65.
- `npm_ok` / RTK : oracle incomplet ; manquent 1 passed (1), 60 passed (60).
- `npm_fail` / RTK : oracle incomplet ; manquent src/cart.test.ts:31:24, expected 119 to be 120, 1 failed.
- `pytest_ok` / RTK : oracle incomplet ; manquent collected 70 items, tests/test_orders.py.
- `pytest_fail` / RTK : oracle incomplet ; manquent test_reject_zero, tests/test_orders.py:48, status=422, status=200.
- `git_diff` / RTK : oracle incomplet ; manquent -    return subtotal + tax, +    return round(subtotal + tax, 2).
- `git_log` / RTK : oracle incomplet ; manquent Maintenance batch 0, Maintenance batch 1, Maintenance batch 2, Maintenance batch 3, Maintenance batch 4, Maintenance batch 5, Maintenance batch 6, Maintenance batch 7, Maintenance batch 8, Maintenance batch 9, Maintenance batch 10, Maintenance batch 11, Maintenance batch 12, Maintenance batch 13, Maintenance batch 14, Maintenance batch 15, Maintenance batch 16, Maintenance batch 17, Maintenance batch 18, Maintenance batch 19, Maintenance batch 20, Maintenance batch 21, Maintenance batch 22, Maintenance batch 23, Maintenance batch 24, Maintenance batch 25, Maintenance batch 26, Maintenance batch 27, Maintenance batch 28, Maintenance batch 29, Maintenance batch 30, Maintenance batch 31, Maintenance batch 32, Maintenance batch 33, Maintenance batch 34, Maintenance batch 35, Maintenance batch 36, Maintenance batch 37, Maintenance batch 38, Maintenance batch 39, Maintenance batch 40, Maintenance batch 41, Maintenance batch 42, Maintenance batch 43, Maintenance batch 44.
- `json_large` / RTK : oracle incomplet ; manquent "id": 143, "state": "rejected", "amount": 4299, "reason": "limit_exceeded", "id": 89, "id": 179.
- `compile_error` / RTK : oracle incomplet ; manquent src/ledger.rs:73:18, error[E0308], expected `i64`, found `String`, atlas-ledger.

## Cas encore derrière un concurrent

Aucun sur les 22 fixtures de ce banc.

## Sources officielles

- [RTK 0.50.0](https://github.com/rtk-ai/rtk/releases/tag/v0.50.0).
- [Headroom, installation et API](https://github.com/headroomlabs-ai/headroom/blob/main/README.md).

## Ce que je n'ai pas pu vérifier

- Réparation d'un test par agent : le sandbox du Codex imbriqué bloque l'essai avant modification.
- Coûts facturés, cache fournisseur et intégrations proxy/CCR en production.
- Informations utiles au-delà des oracles déclarés et généralisation à des sorties réelles non présentes dans ces 22 fixtures synthétiques.
