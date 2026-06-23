# lm-resizer

Faire travailler **Claude Code** et **Codex** avec moins de bruit, moins de
tokens, et plus de contexte utile.

Site : <https://phuetz.github.io/lm-resizer/>

Version anglaise : [README.md](README.md)

`lm-resizer` est né d’un problème très concret : quand on utilise Claude Code,
Codex ou un agent MCP sur un vrai dépôt, une grande partie de la fenêtre de
contexte part dans des logs, des sorties de tests, des diffs, des résultats
`rg`, des JSON énormes et des lignes répétitives.

Le modèle finit par lire trop de bruit et pas assez de signal.

La vocation de `lm-resizer` est simple : **économiser des tokens et préserver le
contexte utile en supprimant, compressant ou offloadant les données dont le
modèle n’a pas besoin pour raisonner**.

Quand un agent exécute `cargo test`, `npm test`, `git diff`, `rg`, des linters,
des package managers ou des appels provider/API, la sortie contient souvent des
milliers de lignes répétitives, peu utiles ou structurellement bruyantes.
Envoyer tout cela au LLM gaspille des tokens, remplit la fenêtre de contexte et
peut cacher la vraie erreur. `lm-resizer` filtre et compresse cette sortie avant
qu’elle n’arrive à l’agent, tout en gardant visibles les erreurs importantes,
les chemins de fichiers, les résumés et les liens de récupération.

**Pourquoi ça compte même avec de grandes fenêtres de contexte.** Des fenêtres
plus grandes (200K, 1M+) ne rendent pas le bruit gratuit — il reste coûteux sur
trois axes : le **coût** (on paie au token, à chaque tour), la **latence** (plus
de tokens = réponses plus lentes) et la **dilution d’attention** (le modèle
raisonne moins bien quand le signal est noyé — « lost in the middle »).
`lm-resizer` vise la **densité de signal**, pas le fait de tenir sous une limite
de taille : garder ce dont l’agent a besoin, retirer le reste, et conserver la
sortie complète récupérable. Il peut aussi compresser de façon **query-aware** :
biaiser la rétention vers les lignes pertinentes à la question en cours quand il
doit abandonner quelque chose.

**Agnostique au provider, validé en réel.** lm-resizer se place devant n'importe
quelle API compatible OpenAI/Anthropic. Vérifié end-to-end (2026-06-23) avec
**Mistral** (`api.mistral.ai`, vraie clé) et **Ollama** (local, `$0`) : une sortie
d'outil bruyante a été compressée par le proxy et le modèle amont a répondu à
travers lui. Tout autre provider compatible OpenAI/Anthropic passe par le même chemin.

## Pourquoi j’ai créé ce projet

Je construis une suite d’outils pour rendre les agents de code réellement
utilisables sur des dépôts importants :

- **Code Explorer** pour donner à l’agent une carte du code ;
- **lm-resizer** pour éviter que l’agent gaspille son contexte ;
- **Code Buddy** pour orchestrer le travail d’agent.

L’objectif n’est pas de faire un outil de plus. L’objectif est de rendre les
sessions Claude Code et Codex plus lisibles, plus longues, plus fiables, et plus
faciles à expliquer à d’autres développeurs.

Si vous avez déjà vu un agent perdre le fil parce qu’il a avalé 20 000 lignes de
logs inutiles, `lm-resizer` répond exactement à ce problème.

## Ce que lm-resizer apporte

| Sans lm-resizer | Avec lm-resizer |
| --- | --- |
| Claude/Codex reçoit des logs bruts | Claude/Codex reçoit le signal utile |
| Le contexte se remplit avec du bruit | Le budget contexte est mieux contrôlé |
| Les erreurs importantes sont noyées | Les erreurs, fichiers et résumés restent visibles |
| Les grosses sorties sont tronquées ou perdues | Le brut peut être récupéré localement via CCR |
| Chaque agent improvise ses propres règles | Une couche commune CLI, hooks, MCP et proxy |

## Complémentarité avec Code Explorer

`lm-resizer` est pensé pour fonctionner avec
[Code Explorer](https://github.com/phuetz/code-explorer). Les deux outils ne
répondent pas au même problème, mais ils se renforcent très bien ensemble.

**Code Explorer** donne à Claude Code, Codex ou à un agent MCP une carte
interrogeable du dépôt : fichiers, symboles, appels, dépendances, impact d’un
changement, zones à modifier. Il évite à l’agent de relire tout le code source
pour comprendre la structure du projet.

**lm-resizer** protège ensuite le budget contexte pendant le travail : sorties
terminal, logs de tests, diffs, résultats `rg`, JSON providers, erreurs
répétitives, payloads MCP/HTTP. Il évite que l’agent consomme son contexte avec
du bruit produit par les commandes et les outils.

Ensemble, ils couvrent les deux grandes sources de gaspillage de contexte :

| Besoin de l’agent | Outil |
| --- | --- |
| Comprendre où se trouvent les responsabilités dans le dépôt | Code Explorer |
| Répondre à “qui appelle quoi ?” ou “qu’est-ce qui casse si je change ça ?” | Code Explorer |
| Exécuter tests, builds, recherches et commandes sans noyer le LLM | lm-resizer |
| Garder les erreurs utiles tout en supprimant les lignes répétitives | lm-resizer |
| Récupérer la sortie brute quand elle est vraiment nécessaire | lm-resizer |

Le workflow typique est :

```bash
# 1. Indexer le dépôt pour donner une carte à l’agent
code-explorer analyze .
code-explorer mcp-install --client both --scope project

# 2. Installer la compression de contexte pour les sorties d’outils
lm-resizer init-native-hooks --client all --project-dir . --force
lm-resizer install-hooks --client codex --project-dir . --force
lm-resizer install-hooks --client claude --project-dir . --force

# 3. Utiliser lm-resizer pour les commandes bruyantes
lm-resizer exec --stream -- cargo test
lm-resizer exec -- git diff
lm-resizer exec -- rg "PaymentService"
```

L’intérêt de les faire fonctionner ensemble est simple :

- l’agent sait **où chercher** grâce à Code Explorer ;
- l’agent reçoit **moins de bruit** grâce à lm-resizer ;
- les requêtes consomment moins de tokens ;
- la fenêtre de contexte reste disponible pour le raisonnement ;
- les grosses sorties restent récupérables via CCR au lieu d’être perdues ;
- Claude Code et Codex peuvent travailler plus longtemps sur un gros dépôt sans
  reconstruire la compréhension du projet à chaque étape.

En résumé :

> **Code Explorer aide l’agent à comprendre le dépôt. lm-resizer aide l’agent à
> ne pas se noyer dans ce que le dépôt produit.**

Ce duo est particulièrement intéressant pour montrer publiquement une approche
plus industrielle des agents de code : on ne se contente pas de demander au LLM
de “deviner”, on lui donne une carte du projet et on filtre ce qui pollue son
raisonnement.

## Usages principaux

- **Hooks Claude Code et Codex** : enregistrer les économies de contexte après
  les commandes d’outils.
- **`lm-resizer exec`** : exécuter une commande et renvoyer une sortie filtrée à
  l’agent.
- **MCP** : exposer des outils de compression, récupération et statistiques aux
  agents compatibles.
- **Proxy HTTP/provider** : compresser ou prévisualiser des payloads
  OpenAI/Anthropic compatibles, ainsi que des formes Bedrock et Vertex.
- **CCR recovery** : offloader les données volumineuses localement et récupérer
  la sortie originale quand l’agent a besoin de la preuve complète.

## Installation / build

```bash
git clone https://github.com/phuetz/lm-resizer
cd lm-resizer
cargo build --release
```

Le binaire de release est généré ici :

```bash
target/release/lm-resizer
```

Sur Windows :

```powershell
cargo build --release
.\target\release\lm-resizer.exe doctor --json
```

## Démarrage rapide avec Claude Code et Codex

Installer les hooks natifs projet pour Claude Code et Codex :

```bash
lm-resizer init-native-hooks --client all --project-dir . --force
```

Installer les blocs d’instructions réversibles dans `AGENTS.md` et/ou
`CLAUDE.md` :

```bash
lm-resizer install-hooks --client codex --project-dir . --force
lm-resizer install-hooks --client claude --project-dir . --force
```

Utiliser `lm-resizer` comme wrapper explicite de commandes :

```bash
lm-resizer exec -- git status
lm-resizer exec --json -- cargo test
lm-resizer exec --stream -- cargo test
```

Analyser des sessions Claude/Codex existantes pour estimer les économies
possibles :

```bash
lm-resizer discover-sessions --agent all --markdown
lm-resizer eval fixtures/sessions --recursive --markdown
```

## CLI

Commandes utiles :

```bash
lm-resizer compress --input tool-output.txt --json
type tool-output.txt | lm-resizer compress --json
lm-resizer batch logs/ --recursive --ext log,json,diff --jobs 8 --json

lm-resizer exec -- git status
lm-resizer exec --stream -- cargo test
lm-resizer rewrite -- git status
lm-resizer rewrite-shell "cargo test && git status"

lm-resizer retrieve <ccr-hash>
lm-resizer stats
lm-resizer stats --markdown
lm-resizer doctor --json
```

`exec` lance la commande, applique des filtres inspirés de RTK pour les familles
bruyantes (`git`, `cargo`, `rg`, listings de dossiers, etc.), puis envoie la
sortie filtrée dans le pipeline de compression normal.

`--stream` est utile pour les commandes longues : la sortie reste visible en
direct, puis `lm-resizer` capture et compresse le résultat à la fin.

`rewrite` et `rewrite-shell` sont des primitives sûres pour construire des
hooks : elles n’exécutent pas la commande cible, elles indiquent seulement
comment la router via `lm-resizer exec -- ...`.

## Filtres projet

`lm-resizer` supporte des filtres déclaratifs TOML pour adapter le nettoyage à
un projet :

```bash
lm-resizer init-filters --profile rust --path .lm-resizer/filters.toml --force
lm-resizer verify-filters --path .lm-resizer/filters.toml --json
lm-resizer trust-filters --path .lm-resizer/filters.toml
lm-resizer audit-filters --path .lm-resizer/filters.toml --review
```

Les filtres intégrés couvrent notamment Terraform/OpenTofu, Docker/Podman,
`systemctl`, Homebrew, `make`, GitHub CLI, Go, .NET, Python, JVM, npm/yarn/pnpm,
Kubernetes et AWS CLI.

Par défaut, les filtres projet doivent être vérifiés et approuvés localement
avant d’être utilisés. C’est volontaire : un filtre peut changer ce que l’agent
voit.

## Récupération des sorties brutes

Quand une sortie est volumineuse ou qu’une commande échoue, `lm-resizer exec`
peut sauvegarder la sortie brute dans le répertoire d’état local et ajouter un
indice du type :

```text
[full output: ccr://sha256/...]
```

Pour récupérer la sortie originale :

```bash
lm-resizer retrieve <ccr-hash>
```

Commandes associées :

```bash
lm-resizer tee list --json
lm-resizer tee read <tee-file-name>
lm-resizer tee purge --all
```

Variables utiles :

- `LM_RESIZER_TEE=0` désactive la sauvegarde des sorties brutes.
- `LM_RESIZER_TRACKING=0` désactive l’historique local des économies.
- `LM_RESIZER_STORE` change le chemin du store CCR.
- `LM_RESIZER_STATE_DIR` change le répertoire d’état.

Chemins par défaut du store CCR :

- Windows : `%LOCALAPPDATA%\lm-resizer\ccr.sqlite3`
- Linux/macOS : `$XDG_STATE_HOME/lm-resizer/ccr.sqlite3` ou
  `$HOME/lm-resizer/ccr.sqlite3`

## MCP

Lancer le serveur MCP stdio :

```bash
lm-resizer mcp
```

Installer la configuration MCP quand le client est supporté :

```bash
lm-resizer install --client claude --scope project
lm-resizer install --client codex --scope global
lm-resizer install --client all --scope project
```

Outils MCP exposés :

- `lm_resizer_compress`
- `lm_resizer_retrieve`
- `lm_resizer_stats`

## HTTP / proxy provider

Lancer le serveur local :

```bash
lm-resizer serve --bind 127.0.0.1:8787
```

Endpoints principaux :

- `GET /health`
- `POST /compress`
- `GET /retrieve/:hash`
- `GET /stats`
- `POST /v1/chat/completions`
- `POST /v1/responses`
- `POST /v1/messages`
- routes Bedrock et Vertex compatibles preview/forwarding

Les routes `/v1/*` utilisent une **compression « live-zone » spécifique au provider** :
les routes OpenAI chat/responses et Anthropic messages passent par les dispatchers
dédiés de `lm-resizer-core::transforms::live_zone` (ciblant les derniers blocs
tool/user et préservant les marqueurs de cache provider), au lieu du parcours de
champs générique réservé aux autres routes.

Avec `--upstream <base-url>` ou `LM_RESIZER_UPSTREAM`, la requête compressée est
transmise au provider amont. Sans upstream, le serveur renvoie une prévisualisation
du payload compressé et des statistiques de compression.

## API Rust

```rust
use lm_resizer_core::LmResizer;

let resizer = LmResizer::new();
let report = resizer.compress(tool_output, "current task");
println!("{}", report.output);
```

`LmResizer` utilise le même pipeline par défaut que le CLI et retourne un
`CompressionReport` stable avec les tailles, les étapes appliquées, les clés CCR
et la sortie compressée.

Voir aussi :

- [docs/API.md](docs/API.md)
- [docs/ABI.md](docs/ABI.md)
- [examples/basic.rs](examples/basic.rs)
- [examples/persistent_store.rs](examples/persistent_store.rs)

## C / WASM

`lm-resizer-core` peut produire des artefacts `rlib`, `cdylib` et `staticlib`.
L’ABI minimale expose :

- `lm_resizer_compress_json(content_ptr, content_len, query_ptr, query_len)`
- `lm_resizer_string_free(ptr)`
- `lm_resizer_alloc(len)`
- `lm_resizer_free(ptr, len)`

`lm_resizer_compress_json` exécute le **pipeline complet par défaut** (SmartCrusher
JSON, compression log/diff/source, offload CCR) — l’ABI C natif comme le build WASM,
identique à celui du CLI (ce n’est pas une simple minification).

Le header C est disponible ici : [include/lm_resizer.h](include/lm_resizer.h).
Le wrapper WASM/npm est dans [packages/wasm](packages/wasm).

La publication npm réelle demande des droits externes (`NPM_TOKEN`, `npm login`
ou trusted publishing configuré). Les scripts de dry-run sont fournis pour
vérifier localement le package sans publier.

## Sécurité et vie privée

- Pas de collecteur de télémétrie activé par défaut.
- Pas de runtime Python requis.
- Les stores CCR, tee et statistiques sont locaux.
- Les filtres projet doivent être vérifiés et approuvés.
- La classification ML Magika/ONNX est **optionnelle et désactivée par défaut**
  (détection déterministe locale, sans runtime ONNX ni modèle).

Pour activer la vraie classification ONNX, il faut **compiler avec la feature
`magika`** *et* poser le flag à l’exécution (sinon le flag est sans effet) :

```bash
cargo build --release --features magika
LM_RESIZER_ENABLE_MAGIKA=1 lm-resizer ml-status --json   # -> "active (ort runtime, bundled Magika model)"
```

Cela utilise le crate officiel [`magika`](https://crates.io/crates/magika) de
Google (modèle `standard_v3_3` embarqué, Apache-2.0). En cas d’erreur du modèle,
la détection retombe sur le détecteur déterministe. ONNX = natif uniquement
(jamais compilé dans le build WASM).

Voir aussi [SECURITY.md](SECURITY.md).

## Vérification release

Linux/macOS :

```bash
./scripts/check-release.sh
./scripts/package-release.sh
```

Windows :

```powershell
powershell -File scripts/check-release.ps1
powershell -File scripts/package-release.ps1
```

Le packaging inclut le binaire, la documentation, les exemples, les fixtures
provider, le wrapper WASM, `release-evidence.json` et `SHA256SUMS`.

## Statut d’implémentation

Implémenté en Rust :

- pipeline de compression et store CCR ;
- compression JSON, logs, diffs, sorties de recherche et code source ;
- CLI, MCP, wrappers de commandes, hooks Claude/Codex et proxy HTTP ;
- filtres TOML intégrés et filtres projet approuvables ;
- récupération de sorties brutes et statistiques d’économies ;
- découverte et évaluation de sessions Claude/Codex ;
- API Rust haut niveau ;
- ABI C/WASM minimale ;
- fixtures provider OpenAI, Anthropic, Bedrock et Vertex ;
- dashboard local optionnel ;
- inspection image, nettoyage de transcripts vocaux et statut ML.

Pas encore fourni :

- publication npm réelle sans validation externe ;
- signature Windows réelle sans certificat de signature ;
- filtres très spécifiques à certains projets au-delà des filtres intégrés ;
- intégrations agent-native plus profondes que les hooks actuels.

## Documentation complémentaire

- Guide Claude/Codex : [docs/CLAUDE_CODEX.md](docs/CLAUDE_CODEX.md)
- API : [docs/API.md](docs/API.md)
- ABI : [docs/ABI.md](docs/ABI.md)
- Release : [docs/RELEASE.md](docs/RELEASE.md)
- Portage : [docs/PORTING.md](docs/PORTING.md)
- Posts réseaux sociaux : [docs/SOCIAL_POSTS.md](docs/SOCIAL_POSTS.md)
