# Performances des grosses sorties

Préparer un binaire optimisé, puis lancer depuis la racine :

```sh
binary=$(python3 bench/real/build.py)
cargo run --release --example perf -- --binary "$binary" --build-proof "${binary}.build.json" --output /tmp/lmr-perf-resultats
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
2 s pour un listing. Le repère de 0,3 s pour `cat` est indicatif : la
relecture indépendante a mesuré jusqu’à 0,361 s et trois campagnes sur cinq
au-dessus du repère. Tous les échantillons sont publiés, même au-dessus. `--runs` règle le nombre d'essais ;
les seuils ne sont pas configurables. Le JSON garde tous les essais et le maximum,
les tailles et SHA-256 des entrées, et le SHA-256 du binaire.

Pour comparer une candidate à l'ancien binaire compilé depuis la branche de base :

```sh
cargo run --release --example perf -- --binary "$binary" --build-proof "${binary}.build.json" --before /tmp/lm-resizer-avant \
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

## Reprise après relecture indépendante

Les cinq campagnes complètes sont publiées dans
`bench/real/reprise1/performance-1.json` à `performance-5.json` : 25 mesures
par gros volume. `performance.json` contient la première campagne, sans
sélection du meilleur résultat. Le champ `threshold_enforced` distingue les
listings (barrière de 2 s) de cat (repère indicatif de 0,3 s).
Les maxima cat dépassent ce repère; les nouveaux choix de compression ont
un coût, détaillé dans le rapport de reprise.

Pour mesurer séparément le parcours disque réel et son enveloppe :

```sh
python3 bench/real/time_listing.py --repo /tmp/lmr-repos/TypeScript \
  --binary target/release/lm-resizer --output /tmp/lmr-listing-reel
```

Chaque paire est séquentielle, avec contrôle du texte affiché à l’octet près.
Les caches disque ne sont pas vidés. `reprise1/listing-live.json` conserve
les cinq paires, sans confondre ces temps avec les temps de rejeu.

## Provenance et isolation après contre-revue Grok2

Le binaire candidat est obligatoire (`--binary`), avec son manifeste
`--build-proof`. Aucun `target/release/lm-resizer` n’est choisi implicitement.
`bench/real/build.py` lit le message `compiler-artifact` de Cargo et ouvre
l’exécutable indiqué; il respecte les cibles/configurations Cargo sans supposer
leur disposition sur disque. Les tests construisent un projet réel avec une
cible donnée par l’environnement puis par la configuration, en laissant un faux
ancien exécutable dans le répertoire par défaut. Le test Python tourne en CI;
la validation du manifeste est un test d’intégration Cargo ordinaire.

Chaque invocation utilise un état neuf conservé dans les résultats, y compris avant/après et
texte/JSON. Les durées ne partagent donc plus SQLite ou tee. Les caches du
système d’exploitation ne sont pas vidés. Les fixtures `code_*` utilisent
explicitement `cat`; aucune comparaison de filtre de langage n’est prétendue.
Les seuils mesurent un plafond absolu, pas un facteur d’accélération garanti
par rapport au binaire avant. Cat inchangé ne crée pas de tee.
