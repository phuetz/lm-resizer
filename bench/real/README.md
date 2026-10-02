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
`environment.json` et `python-freeze.txt` pour l'environnement mesuré. Ne pas
installer les dépendances npm de TypeScript pour reproduire le cas d'échec
`hereby: not found`. Les dépendances Python volontairement incomplètes
produisent des erreurs de collecte : ce banc vérifie leur conservation, il ne
certifie pas la suite FastAPI. Mettre le target Cargo hors des clones.

```bash
cargo build --release --locked
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
code Rust. Il reconstruit les préfixes du format réversible `LMR-LINES/1`.

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
