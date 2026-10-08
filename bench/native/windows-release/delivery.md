# Livraison native : préparation Windows et compression

> **Rejeu de la 0.2.6 (Linux, 8 octobre 2026), par `bench/real/rejouer.sh`** : mêmes 61 captures, `o200k_base`, oracle RTK 0.50.0 épinglé. **Médiane 21,32 %, moyenne 30,25 %** (0.2.5 : 25,18 % / 34,68 % ; avant la correction de `git log` de cette préparation : 30,43 % / 35,14 %), 61/61 bruts récupérés, 61/61 codes du producteur conservés, **37/61** vues strictement égales (0.2.5 : 41/61). Changent `visible-pytest` et `fresh-pytest` : la vue pytest ne liste plus deux fois le même échec (bloc `FAILURES` et ligne de synthèse), alors que l'oracle le fait ; la première passe de 159 à 122 jetons, la seconde de 72 à 62. Changent aussi les six captures `git log` : la vue montre chaque commit au lieu du premier seulement. `ripgrep-log` passe de 82 à 1791 jetons (97,18 → 38,41 % économisés), `fastapi-log` de 80 à 1402 (95,51 → 21,32 %), `compare-log` de 85 à 1904 (97,18 → 36,85 %), `TypeScript-log` de 92 à 1436 et `visible-git-authors` de 76 à 852 (brut rendu, 0 %, la vue ne faisait gagner aucun jeton), `fresh-git-log` (un seul commit) de 62 à 61, ce qui le fait différer de l'oracle d'un jeton. L'oracle garde 65 à 81 jetons pour les cinq captures de plusieurs commits : sa moyenne (32,61 %) dépasse maintenant celle de LM Resizer. Les 53 autres vues sont identiques à la 0.2.5 avant les corrections de la reprise ; Après les corrections de la reprise (vue `cargo test`, rappel `[tee:<id>] lm-resizer tee read <id>`, dernière ligne gardée) vingt captures changent par rapport à la tête `f4d6a8e` : la moyenne passe de 28,97 à **30,25 %**, la médiane reste à 21,32 %, les vues égales à l'oracle de 38 à 37 (`visible-cargo` ne l'est plus : la vue ne double plus le bilan et garde la relance). Le rappel est plus long de 12 jetons quand il s'affiche ; sous 30 % d'économie il disparaît de la vue (`TypeScript-status` 16 → 6 jetons, `fresh-ls` 20 → 9, `fresh-pytest` 62 → 51), et `fresh-jest` passe de 35 à 47. Vues `cargo test` : `ripgrep-test` 244 → 210, `fresh-cargo-test` 159 → 143, `visible-cargo` 205 → 198. (voir [les limites des vues](../../../docs/KNOWN-MISSES.md)).

> **Rejeu de la reprise 9 sur la candidate 0.2.5** (binaire SHA-256 `5fc47fc7c481…`), mêmes 61 captures, `o200k_base`, oracle RTK 0.50.0 épinglé, trois répétitions : [parity-0.2.5-reprise9.json.gz](parity-0.2.5-reprise9.json.gz). Les chiffres Headroom viennent du [rejeu précédent](parity-0.2.5.json.gz) ; Headroom n'a pas été relancé dans cette reprise.
> Le tableau ci-dessous est celui de la livraison du 3 octobre (22,79 % / 31,78 %, 49/61 vues strictement égales) ; **il n'est plus celui de la candidate.** Avec les vues génériques `err`/`test`/`summary` : **médiane 25,18 %, moyenne 34,68 %** (RTK 15,81 % / 32,61 %, Headroom 0,00 % / 1,91 %), **61/61 bruts et codes du producteur**, 52/61 codes identiques à RTK, **41/61 vues strictement égales** avec le banc corrigé (il retire seulement le véritable suffixe tee, sans couper la vue à `filtered_bytes`). La ligne `assert 42 == 43` de `fresh-pytest` et le diagnostic long de `visible-tsc` sont entiers ; l'en-tête d'échec est omis quand un bilan l'indique déjà. Les grands diffs gardent leur fenêtre initiale et leur brut récupérable par `tee read`. `fastapi-test` : 251 jetons contre 40 chez RTK. Résultat rejoué : `target/rtk-parity/reprise9-run4/results.json` (commande `python3 bench/real/parity_rtk.py`, 3 répétitions).


Mesures du 3 octobre 2026, binaire SHA-256 `a7fb175fafa01d6b8acaed69184bbd77d54d706629df4a0d18357cbc1f5c35e8`. Branche native, sans moteur concurrent dans le produit. [Barrière](barriers.md), [détails Windows](README.md), [petites copies](guard.md), [ajouts réversibles](reversible.md). **Recette Windows réelle encore à exécuter.**

| 61 captures, o200k_base | LM avant | LM livré | RTK 0.50.0 | Headroom 0.39.1, API générale |
|---|---:|---:|---:|---:|
| Médiane, tee compris | 21,74 % | **22,79 %** | 15,81 % | 0,00 % |
| Moyenne, tee compris | 31,58 % | **31,78 %** | 32,61 % | 1,91 % |

[Rejeu final](parity-delivery.json.gz) : trois répétitions, 64 pour TSC ; archive de l’oracle épinglée et vérifiée. Les durées absolues du banc ne servent pas de comparaison de vitesse. Le [test séparé entrelacé](timing-delivery.json.gz), sept répétitions sur la même machine, donne **7,68 → 7,70 ms**, RTK **2,92 ms**. Moyennes 13,77 → 13,71 ms, RTK 7,59 ms. Cette reprise maintient la vitesse ; elle ne prétend pas l’améliorer.

**61/61 bruts et 61/61 codes du producteur** ; seulement **52/61 codes identiques à RTK**, qui renvoie zéro dans neuf cas dont le producteur échoue. LM garde le code du producteur. Égalité stricte des vues : **49/61**. Le banc strict termine donc en code 1 ; ce résultat n’est pas présenté comme une parité parfaite. Les douze différences :

| Cas | RTK, jetons | LM, tee inclus | Raison |
|---|---:|---:|---|
| ripgrep-diff | 155 | 170 | En-têtes et patch intégralement réversibles. |
| ripgrep-status | 22 | 17 | Statut propre plus compact, branche et suivi gardés. |
| fastapi-diff | 237 | 248 | En-têtes et patch intégralement réversibles. |
| fastapi-status | 22 | 17 | Statut propre plus compact, branche et suivi gardés. |
| fastapi-test | 40 | 251 | Noms des échecs de collecte conservés dans le résumé. |
| TypeScript-diff | 1805 | 2091 | Tous les contextes et changements conservés. |
| TypeScript-status | 22 | 16 | Statut propre plus compact, branche et suivi gardés. |
| compare-diff | 2539 | 42207 | Tous les fichiers, hunks et contextes conservés ; pas de plafond. |
| visible-docker-logs | 44 | 22 | Rendu natif plus court ; diagnostics conservés. |
| fresh-git-diff | 69 | 82 | En-têtes et patch intégralement réversibles. |
| fresh-tsc | 437 | 437 | Ordre non déterministe de l’oracle, même compte de jetons. |
| fresh-jest | 33 | 35 | Résumé natif de l’échec ; oracle en repli de parseur. |

## Quatre cas ciblés

[Rejeu réel par exec, récupération par tee read](four-final.json), jetons du texte livré :

| Cas | Brut | Avant | Après | RTK | Headroom général |
|---|---:|---:|---:|---:|---:|
| cargo_ok | 837 | 28 | 24 | 24 | 837 |
| dotnet_ok | 111 | 54 | 17 | 21 | 111 |
| git_diff | 195 | 156 | 146 | 125 | 195 |
| compile_error | 106 | 106 | 86 | 78 | 106 |

Cargo corrige aussi l’absence explicite de `0 failed`. Les deux écarts restants sont documentés : inverse intégral du diff, nom du crate/cible dans le bilan de compilation. Les preuves par famille gardent le SHA du binaire testé à cette étape ; le dernier déplacement de fonction pour Clippy ne change pas leur algorithme. Les 61 captures et la vitesse sont rejouées sur le SHA livré ci-dessus.

## Validation et limites

1 355 tests réussis, 3 ignorés ; Clippy workspace/all-targets sans avertissement ; sept tests anti-copie. Corpus public anonyme, dépendances/strings/60 aides sans concurrent visible. Le contrôle descend à 56 jetons contigus et 12 pour des corps complets avec API conservées ; un clone renommé de 15 jetons échoue, aucun faux positif sur l’arbre actuel. Ce contrôle n’est pas une preuve universelle d’originalité.

Windows : les anciennes pertes OEM et recompressions d’échec ne se reproduisent plus sur les contrats natifs. Le tee garde CP850/CRLF ; la vue non UTF-8 affiche des échappements, sans deviner la page de code. CCR pointait encore un intermédiaire : corrigé et récupéré de bout en bout. L’installeur utilise .NET pour SHA/ZIP et un repli Node HTTPS vérifié. Hooks : `cmd` conservé, appel PowerShell explicite et opérateur `&`. **Aucun hôte Windows/PowerShell/Wine disponible** : les tests portables Linux et Node ne prouvent ni Schannel ni l’installation 5.1/7 réelle. Suivre [la recette Windows](../../../docs/WINDOWS.md) avant lancement.

Les capacités de la [matrice 88](../matrix.md) sont un inventaire fonctionnel : les replis génériques ne prouvent pas 88 filtres spécialisés. Les contours sont mesurés seulement pour Rust, Python et TypeScript ; la déduplication sur une répétition contrôlée, sans mesure du cache fournisseur. Aucun SearchCompressor ni DiffCompressor ajouté.
