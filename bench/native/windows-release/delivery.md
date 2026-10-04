# Livraison native : préparation Windows et compression

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
