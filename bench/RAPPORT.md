# Banc comparatif LM Resizer / RTK / Headroom

Sur ces 22 fixtures, LM Resizer est derrière sur 2 cas : dotnet_ok, compile_error. Ces pertes sont conservées dans le classement ; la généralisation hors de ce corpus n'est pas démontrée. Une économie ne compte que si l'oracle est intégralement conservé.

## Versions et méthode

- Checkout `df303349e22e07c368a8dd39259ae75fcbb0084a` ; SHA-256 du binaire LM Resizer `54a27db03c14aabad680d134a45ac9f412a82ab361af9f9c65d846d997a838d7`.
- RTK `0.50.0`, Headroom `0.39.1` avec ONNX Runtime `1.24.4`.
- Tokenizer commun : `tiktoken-rs` `o200k_base` ; mêmes octets de fixture pour tous.
- `compile_error`, `git_diff` et `dotnet_ok` sont des captures réelles dont les sources figurent dans `bench/capture-src/`. Les autres fixtures sont synthétiques.
- RTK suit la route déclarée dans `cases.json` ; LM Resizer utilise `exec` ou `compress --input` ; Headroom utilise son API `compress(messages)`.
- Les chemins HOME/checkout sont normalisés pour le comptage ; les originaux restent sous `target/banc/results/`.
- Le classement n'évalue pas la fidélité du code de sortie de RTK `pipe` ; les codes observés figurent dans `resultats.json`.
- Latence : processus complet, démarrage Python de Headroom compris ; elle dépend de la machine et du cache.
- Preuve HOME : 24 contre 24 jetons bruts, 24 contre 24 normalisés, empreintes égales : true.
- Replis Headroom vers la détection Python : 0/22. Détails des 66 mesures : `bench/resultats.json`.

## Résultats par catégorie

| Catégorie | Cas | LM Resizer : médiane / oracle | RTK : médiane / oracle | Headroom : médiane / oracle |
|---|---:|---:|---:|---:|
| build | 1 | 0.0 % / 100.0 % | 26.4 % / 100.0 % | 0.0 % / 100.0 % |
| code | 6 | 0.0 % / 100.0 % | 0.0 % / 100.0 % | 0.0 % / 100.0 % |
| database | 1 | 93.2 % / 100.0 % | 0.0 % / 100.0 % | 0.0 % / 100.0 % |
| git | 2 | 42.6 % / 100.0 % | 17.9 % / 52.1 % | 0.0 % / 100.0 % |
| infra | 1 | 90.0 % / 100.0 % | 0.0 % / 100.0 % | 0.0 % / 100.0 % |
| json | 1 | 65.0 % / 100.0 % | 0.0 % / 33.3 % | 51.9 % / 100.0 % |
| logs | 1 | 96.3 % / 100.0 % | 95.1 % / 100.0 % | 91.6 % / 100.0 % |
| prose | 1 | 84.5 % / 100.0 % | 0.0 % / 100.0 % | 0.0 % / 100.0 % |
| tests | 8 | 90.3 % / 100.0 % | 0.0 % / 64.0 % | 0.0 % / 100.0 % |
| GLOBAL | 22 | 74.7 % / 100.0 % | 0.0 % / 79.5 % | 0.0 % / 100.0 % |

## Latence et échecs

| Outil | Médiane | Maximum | Échecs techniques |
|---|---:|---:|---:|
| LM Resizer | 15 ms | 127 ms | 0 |
| RTK | 9 ms | 123 ms | 0 |
| Headroom | 361 ms | 563 ms | 0 |

## Gagnants et pertes par cas

Chaque cellule indique économie qualifiée / économie brute / conservation de l'oracle ; le gagnant utilise uniquement l'économie qualifiée.

| Cas | Gagnant qualifié | LM Resizer | RTK | Headroom |
|---|---|---:|---:|---:|
| cargo_ok | LM Resizer, RTK | 97 % qual. / 97 % brut / 100 % oracle | 97 % qual. / 97 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle |
| cargo_fail | LM Resizer | 87 % qual. / 87 % brut / 100 % oracle | 0 % qual. / 93 % brut / 50 % oracle | 0 % qual. / 0 % brut / 100 % oracle |
| dotnet_ok | RTK | 50 % qual. / 50 % brut / 100 % oracle | 81 % qual. / 81 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle |
| dotnet_fail | LM Resizer | 89 % qual. / 89 % brut / 100 % oracle | 87 % qual. / 87 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle |
| npm_ok | LM Resizer | 96 % qual. / 96 % brut / 100 % oracle | 0 % qual. / 99 % brut / 50 % oracle | 0 % qual. / 0 % brut / 100 % oracle |
| npm_fail | LM Resizer | 92 % qual. / 92 % brut / 100 % oracle | 0 % qual. / 97 % brut / 25 % oracle | 0 % qual. / 0 % brut / 100 % oracle |
| pytest_ok | LM Resizer | 94 % qual. / 94 % brut / 100 % oracle | 0 % qual. / 99 % brut / 67 % oracle | 0 % qual. / 0 % brut / 100 % oracle |
| pytest_fail | LM Resizer | 87 % qual. / 87 % brut / 100 % oracle | 0 % qual. / 99 % brut / 20 % oracle | 0 % qual. / 0 % brut / 100 % oracle |
| git_diff | LM Resizer | 47 % qual. / 47 % brut / 100 % oracle | 36 % qual. / 36 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle |
| git_log | LM Resizer | 38 % qual. / 38 % brut / 100 % oracle | 0 % qual. / 98 % brut / 4 % oracle | 0 % qual. / 0 % brut / 100 % oracle |
| docker | LM Resizer | 90 % qual. / 90 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle |
| psql | LM Resizer | 93 % qual. / 93 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle |
| logs | LM Resizer | 96 % qual. / 96 % brut / 100 % oracle | 95 % qual. / 95 % brut / 100 % oracle | 92 % qual. / 92 % brut / 100 % oracle |
| json_large | LM Resizer | 65 % qual. / 65 % brut / 100 % oracle | 0 % qual. / 99 % brut / 33 % oracle | 52 % qual. / 52 % brut / 100 % oracle |
| compile_error | RTK | 0 % qual. / 0 % brut / 100 % oracle | 26 % qual. / 26 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle |
| code_csharp | aucun gain qualifié | 0 % qual. / 0 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle |
| code_rust | aucun gain qualifié | 0 % qual. / 0 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle |
| code_python | aucun gain qualifié | 0 % qual. / 0 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle |
| code_typescript | aucun gain qualifié | 0 % qual. / 0 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle |
| code_go | aucun gain qualifié | 0 % qual. / 0 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle |
| code_java | aucun gain qualifié | 0 % qual. / 0 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle |
| prose | LM Resizer | 84 % qual. / 84 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle | 0 % qual. / 0 % brut / 100 % oracle |

## Faits manquants par outil

- `cargo_fail` / RTK : oracle incomplet ; manquent src/parser.rs:42:9, expected=422 observed=200.
- `npm_ok` / RTK : oracle incomplet ; manquent 1 passed (1).
- `npm_fail` / RTK : oracle incomplet ; manquent src/cart.test.ts:31:24, expected 119 to be 120, 1 failed.
- `pytest_ok` / RTK : oracle incomplet ; manquent tests/test_orders.py.
- `pytest_fail` / RTK : oracle incomplet ; manquent test_reject_zero, tests/test_orders.py:48, status=422, status=200.
- `git_log` / RTK : oracle incomplet ; manquent Maintenance batch 0, Maintenance batch 1, Maintenance batch 2, Maintenance batch 3, Maintenance batch 4, Maintenance batch 5, Maintenance batch 6, Maintenance batch 7, Maintenance batch 8, Maintenance batch 9, Maintenance batch 10, Maintenance batch 11, Maintenance batch 12, Maintenance batch 13, Maintenance batch 14, Maintenance batch 15, Maintenance batch 16, Maintenance batch 17, Maintenance batch 18, Maintenance batch 19, Maintenance batch 20, Maintenance batch 21, Maintenance batch 22, Maintenance batch 23, Maintenance batch 24, Maintenance batch 25, Maintenance batch 26, Maintenance batch 27, Maintenance batch 28, Maintenance batch 29, Maintenance batch 30, Maintenance batch 31, Maintenance batch 32, Maintenance batch 33, Maintenance batch 34, Maintenance batch 35, Maintenance batch 36, Maintenance batch 37, Maintenance batch 38, Maintenance batch 39, Maintenance batch 40, Maintenance batch 41, Maintenance batch 42, Maintenance batch 43, Maintenance batch 44.
- `json_large` / RTK : oracle incomplet ; manquent "id": 143, "state": "rejected", "amount": 4299, "reason": "limit_exceeded", "id": 89, "id": 179.

## Cas encore derrière un concurrent

- `dotnet_ok` : dépasser RTK (81.1 %), avec oracle complet.
- `compile_error` : dépasser RTK (26.4 %), avec oracle complet.

## Sources officielles

- [RTK 0.50.0](https://github.com/rtk-ai/rtk/releases/tag/v0.50.0).
- [Headroom, installation et API](https://github.com/headroomlabs-ai/headroom/blob/main/README.md).

## Ce que je n'ai pas pu vérifier

- Réparation d'un test par agent : le sandbox du Codex imbriqué bloque l'essai avant modification.
- Coûts facturés, cache fournisseur et intégrations proxy/CCR en production.
- Informations utiles au-delà des oracles déclarés et généralisation à des sorties réelles non présentes dans ces 22 fixtures synthétiques.
