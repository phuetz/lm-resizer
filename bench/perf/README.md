# Performances des grosses sorties

Depuis un arbre Git propre et commité, lancer au premier plan :

```sh
cargo run --locked --release --example perf -- \
  --before-ref fix/recette-inconnu-0.2.4-2026-10-02 \
  --output /tmp/lmr-perf-resultats --runs 5
```

Le banc **reconstruit** le CLI de HEAD et celui de `--before-ref`, séquentiellement,
depuis leurs archives Git immuables. Pour chaque version il lance exactement :

```sh
cargo build --locked --release --bin lm-resizer --target-dir <cache>
```

Le CLI est sélectionné seul : l'example ne peut pas unifier ses dev-dependencies
avec celles du binaire mesuré. Construire l'example ci-dessus prépare uniquement
le pilote ; un ancien `target/release/lm-resizer` n'est jamais choisi implicitement.
Le cache est `target`, les exécutables copiés et figés sont dans
`<output>/artifacts/{before,after}`. Les builds finissent avant toute mesure.
Aucun processus n'est lancé en arrière-plan.

Chaque artefact fournit son archive source, les journaux de build et un fichier
`lm-resizer.provenance.json` : commit complet, arbre Git, SHA-256 de l'archive,
SHA-256 du binaire, version du compilateur et sélection exacte des cibles Cargo.
**Chaque mesure**, texte ou JSON, contient le commit et l'empreinte du binaire
exécuté. Cette empreinte est vérifiée avant et après chaque exécution.
Le SHA peut varier avec le compilateur ou les chemins de compilation ; il ne
remplace pas l'identité du commit et de la procédure de construction.

Pour ne construire les artefacts qu'une seule fois, sans mesurer :

```sh
cargo run --locked --release --example perf -- \
  --build-only --before-ref fix/recette-inconnu-0.2.4-2026-10-02 \
  --output /tmp/lmr-perf-construction
# Puis exécuter au premier plan les artefacts vérifiés :
target/release/examples/perf \
  --binary /tmp/lmr-perf-construction/artifacts/after/lm-resizer \
  --before /tmp/lmr-perf-construction/artifacts/before/lm-resizer \
  --output /tmp/lmr-perf-mesures --runs 5
```

`--binary` et `--before` exigent le fichier de provenance correspondant et
rejettent un binaire remplacé ou sans provenance. Les builds concernent toujours
les fichiers suivis du commit : le banc refuse un arbre source modifié.

Rust/Cargo, Git, `tar`, `/bin/sh`, `/bin/cat`, `ls` et `find` sont nécessaires
(Linux). Le répertoire de résultats doit être nouveau ; les preuves existantes
ne sont jamais écrasées. Aucun fournisseur distant ou modèle payant n'est utilisé.

Le banc clone TypeScript en profondeur 1 dans le répertoire temporaire système,
puis vérifie la révision publique figée dans `run.rs`. Un clone de cette révision
peut être fourni avec `--repo`. Les sorties de `LC_ALL=C ls -R` et `find .` sont
capturées une fois, puis figées et identifiées par SHA-256. Le cas `ls-5mb` répète
des listings complets ; un cas agrandi n'est ajouté que si son entrée diffère du
cas réel. `find-real` dépasse déjà 5 Mo : il n'est pas dupliqué sous un autre nom.
Deux `cat` lisent 3 Mo : source `checker.ts`, puis chemins issus de `find`.

Des exécutables shell rejouent ces sorties sous le nom de la commande originale,
pour exercer les mêmes filtres. Le chemin CLI complet est mesuré : processus
froid, capture, filtres, compression, jetons exacts, tee, SQLite et historique.
La marche du système de fichiers est exclue des mesures rejouées ; les vraies
commandes doivent être mesurées séparément en conservant la même provenance.

Le banc **refuse tout essai** atteignant 2 s pour un listing ou 0,3 s pour `cat`.
`--runs` règle le nombre d'essais (3 par défaut) ; les seuils sont fixes.
Médiane et maximum sont enregistrés, mais la médiane ne décide pas du succès.
Ce contrat est volontairement strict : une machine occupée peut faire échouer
une candidate correcte. Comparer hors compilation et charge contrôlable ; garder
les échecs sous charge comme preuves, sans augmenter automatiquement les seuils.

Le corpus comprend 22 fixtures existantes, gros listings, deux sources `cat`,
Unicode/CRLF/doublons, diagnostics, échec, texte vide et absence de LF final.
L'ancien CLI et le nouveau sont aussi exécutés en JSON : JSON entier et stderr
sont comparés octet pour octet, puis le stdout texte au texte original du JSON.
Les codes retour, comptes exacts, filtres, étapes, clés CCR et indications tee
sont donc contrôlés. Chaque entrée/sortie reste dans les résultats.

`--baseline-results <résultats>` permet de réutiliser une référence **version 2**.
Il exige la provenance du binaire d'origine, la correspondance commit/empreinte
pour chaque ancienne mesure, ainsi que les SHA des entrées et sorties originales.
Les anciens résultats sans provenance sont refusés. `baseline_reused=true`
identifie les mesures historiques recopiées ; ce mode ne rejoue pas l'ancien
binaire et ne convient pas à une recette exigeant un nouvel avant/après.

Pour profiler un artefact construit, sans changer son stdout :

```sh
LM_RESIZER_PROFILE=1 LM_RESIZER_STATE_DIR=/tmp/lmr-perf-etat \
  /tmp/lmr-perf-construction/artifacts/after/lm-resizer exec -- ls -R \
  > /tmp/lmr-perf-sortie.txt
```

Les durées par étape vont sur stderr ; l'instrumentation est inactive par défaut.
Associer aussi ce profil au fichier de provenance de l'artefact exécuté.
