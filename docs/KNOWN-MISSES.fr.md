# Limites des vues natives

La migration des filtres est en cours. Les mesures antérieures ne décrivent pas cette version. Voir [le banc](../bench/native/README.md) pour les écarts mesurés, les commandes restantes et la récupération du brut.

## `git log` sans `--stat`

La vue d'un `git log` ordinaire (sans sentinelle ni `--stat`) ne montre que le premier commit : en-tête, auteur, date et titre ; tous les autres sont comptés dans `[+N lines omitted]` et se relisent par `lm-resizer tee read <identifiant>`. Mesure du 8 octobre 2026 sur quatre dépôts, `git log -n 20` : 1 commit visible sur 20, soit 95,7 à 99,2 % de « réduction » obtenus en masquant le reste. Les captures `git log` du banc de 61 cas reposent sur ce comportement, aligné sur celui de l'oracle : le corriger fera baisser les chiffres publiés. Depuis 0.2.6, `git log --stat` affiche chaque commit et chaque fichier (avec leur nombre de lignes, sans les barres de proportion) ; `--oneline` et `--format` montrent quatre lignes puis comptent le reste (mesuré : 4 commits sur 6) et restent à reprendre.
