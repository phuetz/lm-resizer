# Limites des vues natives

La migration des filtres est en cours. Les mesures antérieures ne décrivent pas cette version. Voir [le banc](../bench/native/README.md) pour les écarts mesurés, les commandes restantes et la récupération du brut.

## `git log` : ce qui reste brut

Depuis 0.2.6, la vue d'un `git log` ne fait plus disparaître de commit. Une sortie qui commence par `commit <hash>` (ordinaire, `-n`, `--stat`, `--pretty=short|fuller`, `--decorate`) est rendue commit par commit : en-tête, `Merge:`, `Author:`, `Date:`, titre et trois lignes de corps au plus (le reste est compté dans `[+N message lines omitted]`, les pieds `Signed-off-by` et `Co-authored-by` sont retirés) ; `--stat` ajoute chaque fichier avec son nombre de lignes, sans les barres de proportion. `--oneline`, `--format`, `--graph` et toute sortie sans en-tête `commit` sont rendus bruts : une ligne par commit, rien n'y est cachable sans perdre une entrée. Une vue qui perdrait un en-tête ou n'économiserait aucun jeton est remplacée par le brut. `git log -p` garde sa vue de patch, qui montrait déjà chaque commit.

Limites : un titre ou une ligne de corps est coupé à 100 caractères, une ligne de métadonnées à 120 (le brut complet se relit par `lm-resizer tee read <identifiant>`). Conséquence mesurée le 8 octobre 2026 sur le banc de 61 captures : les cinq captures `git log` de plusieurs commits passent de 91 à 97 % de « réduction » (premier commit seul, comme l'oracle de comparaison, qui le fait toujours) à 0 à 38 %, la médiane de 30,43 à 21,32 % et la moyenne de 35,14 à 28,97 % (30,25 % après les corrections suivantes de la même préparation).
