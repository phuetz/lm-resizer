# Performances des grosses sorties

Préparer un binaire optimisé, puis lancer depuis la racine :

```sh
cargo build --release
cargo run --release --example perf -- --output /tmp/lmr-perf-resultats
```

Rust/Cargo, Git, `/bin/sh`, `/bin/cat`, `ls` et `find` sont nécessaires (Linux).
Le répertoire de résultats doit être nouveau : les preuves existantes ne sont
jamais écrasées. Aucun fournisseur distant ou modèle payant n'est utilisé.

Le banc clone TypeScript en profondeur 1 dans le répertoire temporaire système,
puis vérifie la révision publique figée dans `run.rs`. Un clone existant de cette
révision peut être fourni avec `--repo`. Les sorties de `LC_ALL=C ls -R` et
`find .` sont capturées une fois. Les fixtures de taille supérieure à 5 Mo
répètent des listings complets, sans couper de ligne. Le fichier lu par `cat`
est le préfixe UTF-8 de 3 Mo du véritable `checker.ts` de TypeScript.
Un second cas `cat` lit également 3 Mo de chemins issus de `find`, pour
couvrir les lignes qui commencent par de la ponctuation.

Un petit exécutable shell rejoue chaque sortie figée sous le nom de la commande
originale, pour que les mêmes filtres soient appliqués. Cela mesure le chemin
CLI complet : création d'un processus froid, capture, filtres, compression,
comptage **exact** des jetons, tee, SQLite et historique. Le parcours disque de
`ls`/`find` est exclu de cette mesure rejouée ; le rapport de recette contient
également les mesures des vraies commandes dans le clone.

Le code de sortie est non nul si **un seul** des trois essais par défaut atteint
2 s pour un listing, ou 0,3 s pour `cat`. `--runs` règle le nombre d'essais ;
les seuils ne sont pas configurables. Le JSON garde tous les essais et le maximum,
les tailles et SHA-256 des entrées, et le SHA-256 du binaire.

Pour comparer une candidate à l'ancien binaire compilé depuis la branche de base :

```sh
cargo run --release --example perf -- --before /tmp/lm-resizer-avant \
  --output /tmp/lmr-perf-parite
```

Cette comparaison peut prendre plusieurs minutes à cause de l'ancien algorithme.
Le JSON CLI entier est comparé octet pour octet : il contient le texte, les
comptages exacts, les filtres, les étapes, les clés CCR et les indications tee.
Le stderr, le code retour et le stdout texte sont également contrôlés.
Le corpus comprend les 22 fixtures existantes, les gros listings et le source,
Unicode, CRLF, doublons, diagnostics, échec, texte vide et absence de LF final.
Chaque entrée et chaque sortie reste disponible dans le répertoire de résultats.

Pour profiler une vraie commande sans modifier son stdout :

```sh
LM_RESIZER_PROFILE=1 LM_RESIZER_STATE_DIR=/tmp/lmr-perf-etat \
  target/release/lm-resizer exec -- ls -R > /tmp/lmr-perf-sortie.txt
```

Pour réutiliser des preuves originales déjà enregistrées, fournir
`--baseline-results /tmp/lmr-perf-parite` à la place de `--before` ; les
SHA-256 de toutes les entrées doivent correspondre.

Les durées par étape sont écrites sur stderr. L'instrumentation est inactive
par défaut. Comparer les performances hors compilation/tests concurrents.

## Branche combinant performance et nouveaux filtres

La comparaison `--before` impose une sortie identique; elle est pertinente pour
un correctif de performance seul. Les vues réversibles modifient volontairement
le format : utiliser `bench/real/run.py` pour leur oracle de conservation, et le
banc présent sans `--before` pour les seuils temporels. Le relevé intégré publié
est dans `bench/real/performance.json` (cinq essais par gros volume).
