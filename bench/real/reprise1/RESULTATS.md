> **Archive remplacée pour les annonces publiques.** Ces contrôles décodaient les vues et ne prouvaient pas la visibilité des faits. Voir le [banc corrigé](../visible/RESULTATS.md).

# Banc réel : avant / après / RTK

30 cas principaux, trois dépôts publics épinglés; cinq cas de comparaison supplémentaires.
tiktoken 0.14.0; comptes réels des sorties, marqueurs inclus; médianes et moyennes non pondérées.
Une économie reste comptée même si des faits disparaissent. Aucun résultat « qualifié » remis à zéro.

| Encodage | Mesure | LMR avant | LMR après | RTK 0.50.0 |
|---|---|---:|---:|---:|
| cl100k_base | médiane | 3.74 % | 7.38 % | 43.28 % |
| cl100k_base | moyenne | 13.21 % | 18.34 % | 47.33 % |
| o200k_base | médiane | 3.75 % | 7.38 % | 43.21 % |
| o200k_base | moyenne | 13.26 % | 18.47 % | 47.41 % |

Sources LMR mesurées : `fd22a8fd160e2b831bcf3992b2733aa052ec0404`.
SHA-256 du binaire LMR après : `b2caff10d8fc8781e0ecfb9f31535f185f853037f25d231867d0c04720115256`.
Les commits documentaires ultérieurs ne changent pas cette provenance. Les sources et la commande de compilation sont dans le JSON.


Faits LMR conservés : **160,014/160,014** après; 0 faits non reconnus avant. 25/25 cas avec restitution tee vérifiés à l’octet près; les autres cas ne créent pas de tee. Codes de sortie conservés : 35/35.

## Détail o200k (les deux encodages sont dans les JSON)

| Cas | Brut | LMR avant | LMR après | RTK | LMR avant s | LMR après s | Faits absents LMR après |
|---|---:|---:|---:|---:|---:|---:|---:|
| ripgrep-log | 2,973 | 2,836 | 2,803 | 101 | 0.173 | 0.125 | 0 |
| ripgrep-diff | 228 | 228 | 228 | 156 | 0.125 | 0.136 | 0 |
| ripgrep-status | 23 | 23 | 23 | 22 | 0.086 | 0.160 | 0 |
| ripgrep-show | 104 | 104 | 104 | 104 | 0.094 | 0.114 | 0 |
| ripgrep-ls | 837 | 656 | 637 | 245 | 0.137 | 0.123 | 0 |
| ripgrep-recursive | 1,639 | 1,595 | 1,595 | 285 | 0.120 | 0.147 | 0 |
| ripgrep-find | 3,695 | 2,771 | 2,706 | 596 | 0.121 | 0.102 | 0 |
| ripgrep-grep | 243 | 243 | 243 | 243 | 0.105 | 0.130 | 0 |
| ripgrep-cat | 63,937 | 51,864 | 48,174 | 63,937 | 0.127 | 0.221 | 0 |
| ripgrep-test | 5,081 | 325 | 318 | 232 | 0.133 | 0.089 | 0 |
| fastapi-log | 2,172 | 1,419 | 1,346 | 88 | 0.198 | 0.086 | 0 |
| fastapi-diff | 279 | 279 | 279 | 237 | 0.114 | 0.061 | 0 |
| fastapi-status | 23 | 23 | 23 | 22 | 0.092 | 0.159 | 0 |
| fastapi-show | 107 | 107 | 107 | 107 | 0.088 | 0.137 | 0 |
| fastapi-ls | 529 | 529 | 487 | 136 | 0.089 | 0.138 | 0 |
| fastapi-recursive | 35,310 | 31,274 | 21,292 | 137 | 0.134 | 0.152 | 0 |
| fastapi-find | 61,723 | 48,613 | 42,243 | 622 | 0.162 | 0.095 | 0 |
| fastapi-grep | 759 | 708 | 708 | 722 | 0.123 | 0.090 | 0 |
| fastapi-cat | 223,103 | 218,244 | 167,588 | 223,103 | 0.166 | 0.251 | 0 |
| fastapi-test | 100,772 | 44,483 | 26,232 | 40 | 0.127 | 0.173 | 0 |
| TypeScript-log | 1,628 | 1,517 | 1,517 | 88 | 0.113 | 0.139 | 0 |
| TypeScript-diff | 2,174 | 2,111 | 1,852 | 1,805 | 0.105 | 0.135 | 0 |
| TypeScript-status | 23 | 23 | 23 | 22 | 0.093 | 0.100 | 0 |
| TypeScript-show | 205 | 205 | 205 | 205 | 0.210 | 0.104 | 0 |
| TypeScript-ls | 745 | 651 | 601 | 237 | 0.087 | 0.134 | 0 |
| TypeScript-recursive | 616,572 | 397,296 | 397,296 | 161 | 0.165 | 0.364 | 0 |
| TypeScript-find | 1,361,069 | 1,029,234 | 587,128 | 601 | 0.436 | 0.617 | 0 |
| TypeScript-grep | 25,868 | 21,662 | 21,198 | 19,347 | 0.085 | 0.130 | 0 |
| TypeScript-cat | 597,267 | 591,931 | 583,734 | 597,267 | 0.277 | 0.571 | 0 |
| TypeScript-test | 31 | 31 | 31 | 14 | 0.103 | 0.107 | 0 |
| compare-log | 3,083 | 2,937 | 2,905 | 75 | 0.097 | 0.085 | 0 |
| compare-diff | 43,405 | 40,301 | 37,676 | 2,539 | 0.094 | 0.118 | 0 |
| compare-grep | 2,966 | 2,645 | 2,645 | 1,795 | 0.082 | 0.126 | 0 |
| compare-find | 14,203 | 11,224 | 9,905 | 775 | 0.105 | 0.126 | 0 |
| compare-cat | 535,949 | 507,181 | 474,299 | 535,949 | 0.223 | 0.342 | 0 |

## Interprétation et limites

- Les temps LMR incluent le CLI, un enfant de rejeu, la compression, les comptes exacts, tee et SQLite. Une seule passe par cas.
- RTK utilise une capture identique via `pipe` lorsque son filtre existe; les autres cas sont relancés. Les modes et temps séparés sont dans `rtk.json`.
- Le filtre RTK pipe ne propage pas le code de la commande initiale. On ne compare pas ce code à celui du rejeu LMR.
- L’oracle décode les regroupements grep/find RTK; des reformulations restent non reconnues. Son compteur n’est pas une preuve universelle de pertes sémantiques.
- Les lignes grep et les chemins sont vérifiés avec leur multiplicité; les fichiers cat à l’octet UTF-8 près, espaces et fin de ligne inclus.
- Les réussites individuelles peuvent être résumées; chaque diagnostic et chaque compteur de résultat restent exigés.
- Les captures de test ne sont pas des certifications des projets : dépendances Python incomplètes, erreurs réelles conservées, npm sans hereby.
- Le rapport Grok initial ne donnait pas les commits : ce banc fixe ses propres entrées, pas une reproduction au jeton près de cette première mesure.
- Aucune mesure de facture fournisseur, qualité des décisions des agents ou performance universelle.

## Variabilité des latences (cinq campagnes, 25 échantillons par cas)

| Cas | Médiane s | Minimum s | Maximum s | Seuil bloquant |
|---|---:|---:|---:|---|
| ls-real | 0.266 | 0.180 | 0.462 | 2 s |
| ls-5mb | 0.337 | 0.262 | 0.495 | 2 s |
| find-real | 0.476 | 0.351 | 0.673 | 2 s |
| find-5mb | 0.438 | 0.332 | 0.628 | 2 s |
| cat-paths-3mb | 0.380 | 0.305 | 0.646 | aucun; repère 0,3 s dépassé |
| cat-3mb | 0.410 | 0.366 | 0.753 | aucun; repère 0,3 s dépassé |

Les fichiers `performance-1.json` à `performance-5.json` conservent tous les essais. Le premier relevé est également publié comme `../performance.json`.

La vraie commande enveloppée `ls -R` : médiane 0,445 s, maximum 0,498 s, cinq essais. `listing-live.json` distingue le parcours natif (médiane 0,096 s) du total enveloppé. Aucun cache disque vidé.
