# Banc réel : avant / après / RTK

30 cas principaux, trois dépôts publics épinglés; cinq cas de comparaison supplémentaires.
tiktoken 0.14.0; comptes réels des sorties, marqueurs inclus; médianes et moyennes non pondérées.
Une économie reste comptée même si des faits disparaissent. Aucun résultat « qualifié » remis à zéro.

| Encodage | Mesure | LMR avant | LMR après | RTK 0.50.0 |
|---|---|---:|---:|---:|
| cl100k_base | médiane | 0.00 % | 3.74 % | 43.28 % |
| cl100k_base | moyenne | 7.94 % | 13.21 % | 47.33 % |
| o200k_base | médiane | 0.00 % | 3.75 % | 43.21 % |
| o200k_base | moyenne | 7.96 % | 13.26 % | 47.41 % |

Faits LMR conservés : **160,014/160,014** après; 359 faits non reconnus avant. 24 restitutions tee vérifiées à l’octet près. Codes de sortie conservés : 35/35.

## Détail o200k (les deux encodages sont dans les JSON)

| Cas | Brut | LMR avant | LMR après | RTK | LMR avant s | LMR après s | Faits absents LMR après |
|---|---:|---:|---:|---:|---:|---:|---:|
| ripgrep-log | 2,973 | 2,468 | 2,836 | 101 | 0.290 | 0.098 | 0 |
| ripgrep-diff | 228 | 115 | 228 | 156 | 0.181 | 0.078 | 0 |
| ripgrep-status | 23 | 23 | 23 | 22 | 0.190 | 0.080 | 0 |
| ripgrep-show | 104 | 104 | 104 | 104 | 0.162 | 0.104 | 0 |
| ripgrep-ls | 837 | 837 | 656 | 245 | 0.196 | 0.085 | 0 |
| ripgrep-recursive | 1,639 | 1,659 | 1,595 | 285 | 0.128 | 0.093 | 0 |
| ripgrep-find | 3,695 | 3,695 | 2,771 | 596 | 0.146 | 0.086 | 0 |
| ripgrep-grep | 243 | 243 | 243 | 243 | 0.155 | 0.101 | 0 |
| ripgrep-cat | 63,937 | 63,937 | 51,864 | 63,937 | 0.167 | 0.100 | 0 |
| ripgrep-test | 5,081 | 5,081 | 325 | 232 | 0.135 | 0.098 | 0 |
| fastapi-log | 2,172 | 1,732 | 1,419 | 88 | 0.136 | 0.092 | 0 |
| fastapi-diff | 279 | 100 | 279 | 237 | 0.179 | 0.061 | 0 |
| fastapi-status | 23 | 23 | 23 | 22 | 0.140 | 0.077 | 0 |
| fastapi-show | 107 | 107 | 107 | 107 | 0.167 | 0.073 | 0 |
| fastapi-ls | 529 | 529 | 529 | 136 | 0.149 | 0.087 | 0 |
| fastapi-recursive | 35,310 | 35,329 | 31,274 | 137 | 0.241 | 0.127 | 0 |
| fastapi-find | 61,723 | 61,723 | 48,613 | 622 | 0.331 | 0.088 | 0 |
| fastapi-grep | 759 | 747 | 708 | 722 | 0.193 | 0.074 | 0 |
| fastapi-cat | 223,103 | 223,103 | 218,244 | 223,103 | 0.644 | 0.107 | 0 |
| fastapi-test | 100,772 | 100,772 | 44,483 | 40 | 0.300 | 0.093 | 0 |
| TypeScript-log | 1,628 | 1,226 | 1,517 | 88 | 0.158 | 0.109 | 0 |
| TypeScript-diff | 2,174 | 1,790 | 2,111 | 1,805 | 0.170 | 0.086 | 0 |
| TypeScript-status | 23 | 23 | 23 | 22 | 0.141 | 0.098 | 0 |
| TypeScript-show | 205 | 205 | 205 | 205 | 0.165 | 0.104 | 0 |
| TypeScript-ls | 745 | 745 | 651 | 237 | 0.145 | 0.095 | 0 |
| TypeScript-recursive | 616,572 | 616,593 | 397,296 | 161 | 43.628 | 0.206 | 0 |
| TypeScript-find | 1,361,069 | 1,361,069 | 1,029,234 | 601 | 65.011 | 0.583 | 0 |
| TypeScript-grep | 25,868 | 25,868 | 21,662 | 19,347 | 1.888 | 0.172 | 0 |
| TypeScript-cat | 597,267 | 597,267 | 591,931 | 597,267 | 2.215 | 0.284 | 0 |
| TypeScript-test | 31 | 17 | 31 | 14 | 0.295 | 0.121 | 0 |
| compare-log | 3,083 | 2,552 | 2,937 | 75 | 0.350 | 0.076 | 0 |
| compare-diff | 43,405 | 40,470 | 40,301 | 2,539 | 6.704 | 0.113 | 0 |
| compare-grep | 2,966 | 2,966 | 2,645 | 1,795 | 0.348 | 0.074 | 0 |
| compare-find | 14,203 | 14,203 | 11,224 | 775 | 0.348 | 0.086 | 0 |
| compare-cat | 535,949 | 535,949 | 507,181 | 535,949 | 0.708 | 0.204 | 0 |

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
