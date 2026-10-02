# Banc réel du 02/10/2026

Ce banc complète les anciennes fixtures synthétiques de `bench/`. Il utilise
les trois dépôts du rapport Grok : ripgrep, FastAPI et TypeScript. Les commits
et URLs sont dans `repos.json`. Le rapport initial ne publie pas ses commits
ni tous ses arguments : ce banc reprend ses familles de commandes, et **ne
prétend pas reproduire ses entrées au jeton près**.

## Reproduction

Installer Python, `tiktoken==0.14.0`, Git, Rust 1.96 pour ripgrep et pytest.
Créer trois clones dans un dossier de travail externe au dépôt :

```bash
python3 - <<'PY'
import json, pathlib, subprocess
root = pathlib.Path('/tmp/lmr-repos')
root.mkdir(exist_ok=True)
for repo in json.loads(pathlib.Path('bench/real/repos.json').read_text()):
    destination = root / repo['name']
    subprocess.run(['git', 'clone', '--depth', str(repo['depth']), repo['url'], str(destination)], check=True)
    subprocess.run(['git', '-C', str(destination), 'fetch', '--depth', str(repo['depth']), 'origin', repo['commit']], check=True)
    subprocess.run(['git', '-C', str(destination), 'checkout', '--detach', repo['commit']], check=True)
PY
```

La préparation des dépendances de test est séparée de la mesure. Voir
`environment.json` et `python-freeze.txt` pour l'environnement mesuré.
Après le clonage, préparer un environnement isolé (Python 3.11 pour le rejeu
publié) :

```bash
python3.11 -m venv /tmp/lmr-bench-env
/tmp/lmr-bench-env/bin/pip install -r bench/real/python-freeze.txt
/tmp/lmr-bench-env/bin/pip install --no-deps -e /tmp/lmr-repos/fastapi
rustup toolchain install 1.96.0 --profile minimal
# Sorties de compilation hors du clone : find/ls ne doivent pas voir target/.
CARGO_TARGET_DIR=/tmp/lmr-real-cargo RUSTUP_TOOLCHAIN=1.96.0 \
  cargo test --no-run --locked --manifest-path /tmp/lmr-repos/ripgrep/Cargo.toml
export CARGO_TARGET_DIR=/tmp/lmr-real-cargo
export RUSTUP_TOOLCHAIN=1.96.0
export PATH="/tmp/lmr-bench-env/bin:$PATH"
```

Le manifeste LM Resizer reste compilé avec son toolchain épinglé : employer
`cargo +1.86.0 build --release --locked` dans ce shell. La préparation ci-dessus
ne lance pas les tests : les captures restent les vraies commandes mesurées.
Les fichiers temporaires de pytest et l'état de Git peuvent faire varier une
capture neuve; les SHA-256 figent les entrées du rejeu avant/après local.
 Ne pas
installer les dépendances npm de TypeScript pour reproduire le cas d'échec
`hereby: not found`. Les dépendances Python volontairement incomplètes
produisent des erreurs de collecte : ce banc vérifie leur conservation, il ne
certifie pas la suite FastAPI. Mettre le target Cargo hors des clones.

```bash
cargo +1.86.0 build --release --locked --target-dir target
cp target/release/lm-resizer /tmp/lm-resizer-before
python3 bench/real/run.py capture --repos /tmp/lmr-repos --work /tmp/lmr-results
python3 bench/real/run.py replay --repos /tmp/lmr-repos --work /tmp/lmr-results --binary /tmp/lm-resizer-before --label before
# Modifier et recompiler LM Resizer, puis :
python3 bench/real/run.py replay --repos /tmp/lmr-repos --work /tmp/lmr-results --binary target/release/lm-resizer --label after
```

Le rejeu **échoue** si un fait ou un code de sortie disparaît. Le rejeu avant
est donc susceptible de sortir en code 1 : c'est un résultat de contrôle.
Un dossier existant n'est jamais écrasé. Les captures gardent les deux flux,
leur SHA-256, le code et la durée de la commande réelle. `exec` rejoue les
flux avec un enfant portant le nom du programme original : mêmes arguments,
même routage, mêmes erreurs et même code, sans variation des durées imprimées
par les tests. Les durées `exec_seconds` comprennent le démarrage du CLI, la
compression, tiktoken interne, SQLite, tee et l'enfant Python. `added_seconds`
soustrait le temps du même enfant sans LM Resizer. Ce ne sont pas des mesures
de temps total de la suite de tests ni de facture API.

## Faits exigés

Chaque cas produit sa **liste exhaustive** `<cas>.facts.json`, sa vue et ses
métadonnées. Les listes et captures volumineuses restent dans le dossier de
travail, les mesures compactes sont versionnées. L'oracle est indépendant du
code Rust. Il reconstruit les préfixes du format réversible `LMR-LINES/2`.

- grep : chaque tuple (fichier, numéro de ligne, contenu exact), et toute
  ligne non reconnue. Aucun plafond de fichiers, lignes ou longueur.
- find : chaque chemin complet.
- ls -R : chaque dossier, y compris vide, et chaque couple dossier/nom.
- cat : contenu **exact**, ordre, espaces et saut de ligne final inclus.
- git : toutes les lignes non vides, dates, auteurs, hash complets, corps et
  hunks inclus. Aucune dépendance à une poignée de mots clés.
- tests : toutes les lignes hors décorations de progression reconnues et
  lignes de réussite Cargo ; diagnostics, piles, avertissements, codes et
  compteurs restent exigés. Les noms de tests réussis peuvent être résumés.

Les médianes et moyennes sont les économies **signées** par commande sur les
30 cas principaux non vides, avec `cl100k_base` et `o200k_base` réels. Les cinq
cas supplémentaires du comparatif ne changent pas la médiane principale.
Les marqueurs de récupération et de flux font partie de la sortie comptée.
Un oracle incomplet est signalé séparément : jamais remplacé par une économie
« qualifiée » zéro dans les statistiques de réduction brute.

Les checks de conservation ne démontrent pas que tout utilisateur jugera la
vue aussi lisible que le brut. Les fichiers sans motif répétitif peuvent
rester identiques. Aucun résultat ne justifie une promesse universelle.

## Vue réversible

`LMR-LINES/2` conserve l'ordre et chaque octet UTF-8 des lignes : `@` suivi
d'un préfixe JSON définit le préfixe des lignes littérales suivantes; `&N`
réutilise la ligne décodée N (indice zéro); `=N` répète N fois la dernière
ligne; `!1`/`!0` indique la présence/absence du saut final. Les lignes
littérales commençant par `@`, `&`, `=`, `!` ou `\` sont échappées avec `\`.
Les références désignent des lignes intégrales, jamais des erreurs omises.
Le décodeur indépendant est `expand()` dans `run.py`; il accepte aussi la
version 1 de la première session. Le brut est conservé dans tee dès que la
vue change. La sélection exige un gain mesuré o200k supérieur au coût prévu
du marqueur de récupération. Une sortie peu répétitive peut rester brute.

Les lignes de tests Cargo réussis peuvent être remplacées par les compteurs
de la suite, mais les blocs de diagnostic restent intégraux et reconstructibles.
La comparaison stricte de toutes les lignes Git garde les dates et les hunks.

## RTK et limites de la comparaison

Installer le binaire officiel v0.50.0 avec son script et le checksum obligatoire,
puis (adapter uniquement le chemin du binaire) :

```bash
python3 bench/real/compare_rtk.py --work /tmp/lmr-results \
  --repos /tmp/lmr-repos --binary /tmp/rtk/rtk
python3 -m unittest discover -s bench/real -p test_oracle.py
```

Le champ `mode` distingue un filtre `rtk pipe` sur la **capture identique** d'une
commande native relancée. Le mode pipe ne comprend pas l'exécution de la commande
et ne propage pas son code d'échec; ses durées ne sont pas une comparaison du temps
total des suites. Les listes grep/find RTK sont dégroupées avant comparaison.
`oracle_missing_count` compte les faits non reconnus, pas une preuve automatique
de perte sémantique : RTK peut reformuler. Les changements d'indentation de grep
sont comptés séparément dans `search_whitespace_changed`. Les compteurs d'erreurs,
hash complets et noms absents restent des exemples vérifiables de perte.

Les médianes RTK incluent les commandes disponibles, même si l'oracle échoue;
on ne remplace jamais leur économie réelle par zéro. Les compteurs RTK internes
ne servent pas au calcul : Python tiktoken compte les sorties des deux outils.
Le comparatif n'exécute ni code téléchargé par un agent ni appel à un modèle.

`lm-resizer expand -i vue.txt` reconstruit le texte sans tee. Son décodeur refuse
les références invalides, les vues de plus de 512 Mio ou d'un million de lignes;
le tee conserve l'original, indépendamment de cette limite du décodeur CLI.

Le rejeu compare aussi les deux comptes o200k du CLI au tokenizer Python
indépendant (`cli_exact_counts_verified`). Les optimisations de comptage ne
peuvent donc pas transformer silencieusement les mesures en estimations.

Pour régénérer le tableau à partir des trois JSON :

```bash
python3 bench/real/report.py --before bench/real/before.json \
  --after bench/real/after.json --rtk bench/real/rtk.json \
  --output /tmp/lmr-tableau.md
```
