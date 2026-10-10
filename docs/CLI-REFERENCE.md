# Référence de la ligne de commande

Cette page liste **toutes** les commandes et sous-commandes, **toutes** les
options longues et les variables d'environnement affichées par
`lm-resizer --help` et par l'aide de chaque commande ; le script
`scripts/check-cli-reference.sh` échoue si l'une d'elles manque ici. Chaque
ligne ajoutée à la version 0.2.4 a été rejouée ; ce qui n'a pas pu l'être est
dit en fin de page. Pour le détail d'une commande, `lm-resizer <commande> --help`
fait foi.

`lm-resizer -V` (ou `--version`) affiche la version. `lm-resizer help <commande>`
équivaut à `lm-resizer <commande> --help`. `lm-resizer help --help` est refusé
(`unrecognized subcommand '--help'`) : utiliser `lm-resizer --help`.

## Commandes

Toutes les commandes de premier niveau, puis les sous-commandes de `tee`.

| Commande | Rôle (texte de `--help`) |
| --- | --- |
| `observe` | Summarize an arbitrary command; the complete output stays recoverable |
| `err` | Keep diagnostic paragraphs from any failed command |
| `test` | Summarize test output from any command |
| `summary` | Summarize any command while retaining unknown diagnostics |
| `proxy` | Execute literally while retaining raw recovery and measured history |
| `run` | Execute a literal shell command with recovery and measured history |
| `json` | Show a compact JSON schema (all input bytes remain recoverable) |
| `outline` | Show callable signatures using a fresh local syntax index; bodies stay in tee |
| `read` | Read a file literally, optionally showing only its first lines |
| `deps` | List direct dependencies from local manifests |
| `env` | Show sorted environment variables with secrets redacted |
| `format` | Run a formatter through the native capture and view engine |
| `dedup` | Fold exact repeated tool blocks inside a supplied JSON message window |
| `hook-audit` | Count locally recorded hook rewrites by client |
| `config` | Show effective local storage and capture configuration |
| `pipe` | Filter stdin without executing any command; accepts native pipe filter names |
| `expand` | Expand a reversible view from stdin or a file, without accessing tee |
| `compress` | Compress stdin or a file and persist originals for CCR retrieval |
| `smart` | Summarize a source file with indexed Code Explorer symbols when available |
| `batch` | Compress many files in parallel |
| `exec` | Execute a command with the native command view and recoverable raw output |
| `tool-output` | Filter output already captured by a host or benchmark; never execute the command |
| `rewrite` | Show how a shell command would be routed through `lm-resizer exec` |
| `rewrite-shell` | Rewrite a full shell command line without executing it |
| `retrieve` | Retrieve an original payload by CCR hash [aliases: recall] |
| `share` | Save a named, compressed handoff for other agents sharing this store |
| `shared-get` | Read a named handoff, compressed by default or verbatim with --full |
| `shared-list` | List names of handoffs in the shared store |
| `stats` | Show CCR store statistics [aliases: gain] |
| `image` | Inspect image payload size and dimensions for context-budget decisions |
| `voice` | Analyze or clean voice transcript filler words |
| `ml-status` | Report optional ML classifier/model configuration |
| `tee` | Manage raw output recovery files created by exec |
| `trust-filters` | Trust a project-local `.lm-resizer/filters.toml` file |
| `list-trusted-filters` | List trusted project filter files |
| `untrust-filters` | Remove a project filter file from the trust registry |
| `audit-filters` | Show a readable audit of a TOML filter file before trusting it |
| `verify-filters` | Validate a TOML filter file and run its inline tests |
| `init-filters` | Create a starter project filter file with inline tests |
| `sanitize-provider-fixture` | Sanitize a real provider payload into a shareable fixture JSON file |
| `discover` | Analyze logs/session files for commands that lm-resizer exec can reduce |
| `discover-sessions` | Discover compressible command output in known Claude/Codex session stores |
| `eval` | Run a lightweight evaluation harness over session/log fixtures |
| `learn` | Mine sessions/history and propose durable AGENTS.md / CLAUDE.md guidance |
| `init-hooks` | Generate local hook helper scripts for agent command rewriting |
| `init-native-hooks` | Install native agent hooks that call `lm-resizer hook` [aliases: init] |
| `hook` | Native Codex/Claude hook handler. Reads event JSON from stdin and never blocks |
| `init-shims` | Generate opt-in PATH shims that automatically route known commands through exec |
| `install-hooks` | Install reversible project agent instructions for hook helpers |
| `uninstall-hooks` | Remove generated lm-resizer hook instructions from project agent files |
| `doctor` | Diagnose local lm-resizer setup |
| `mcp` | Run a minimal MCP stdio server |
| `mcp-proxy` | Relay an MCP stdio server and compress successful tool text results |
| `install` | Install lm-resizer as an MCP server for common agent clients |
| `uninstall` | Remove the lm-resizer MCP server entry written by `install`, keeping the rest of each file |
| `serve` | Run a small HTTP API |
| `wrap` | Start the local proxy, then launch an agent through it |
| `tee list` | List raw output recovery files |
| `tee read` | Print one raw output recovery file by filename or path |
| `tee purge` | Delete raw output recovery files |

Détail des commandes qui n'ont pas d'autre page de documentation :

| Commande | Précisions rejouées |
| --- | --- |
| `smart <fichier> [--ast] [-q <requête>] [--json] [--store <chemin>]` | Sans `--ast`, compression existante avec conseil Code Explorer quand disponible. Avec `--ast`, résumé syntaxique local Rust, Python, TypeScript ou JavaScript, signatures et numéros de ligne, corps omis. |
| `share <nom> [-i <fichier>] [-q <requête>] [--json] [--store <chemin>]` | Enregistre une passation nommée, compressée. Lit stdin sans `-i`. Affiche le nom (ou, avec `--json`, `key`, `original_bytes`, `shared_bytes`). |
| `shared-get <nom> [--full] [--store <chemin>]` | Compressée par défaut ; texte d'origine, octets identiques, avec `--full`. |
| `shared-list [--store <chemin>]` | Liste les noms des passations du store. |
| `list-trusted-filters [--json]` | Liste les fichiers de filtres approuvés par `trust-filters`. |
| `untrust-filters [--path <fichier>] [--json]` | Retire un fichier du registre de confiance (défaut : `.lm-resizer/filters.toml`) ; `removed: false` s'il n'y était pas. |

`smart --ast` conserve l'ordre des déclarations, les imports, les types et leurs
membres, les fonctions/méthodes avec leur signature et la première ligne de
documentation. Les constantes visibles sont listées sans leur valeur : visibilité
explicite en Rust, exports en JS/TS, noms en majuscules en Python. Les déclarations
contenues dans un corps de fonction ne sont pas parcourues. Les macros Rust ne
sont pas développées. L'analyse est syntaxique et ne résout pas les imports ni les
types.

Le mode AST réussi n'utilise ni modèle, ni réseau, ni index Code Explorer, ni
store. `--query` et `--store` restent acceptés et s'appliquent seulement au repli.
Une extension inconnue ou une erreur syntaxique revient au traitement existant,
avec une ligne `[smart AST: repli …]` en tête de la sortie. En JSON, cette ligne
figure dans `output`. Le JSON AST réussi mesure les octets et annonce
`token_count_method: "approximate"`, `tokenizer: "bytes/4"` : **octets ÷ 4 est une
approximation de jetons**, pas une tokenisation. Les petits fichiers déjà limités
à des signatures peuvent produire un résumé plus long que l'entrée.

## Options

| Option | Commande | Effet |
| --- | --- | --- |
| `-q`, `--query <texte>` | `compress`, `exec`, `smart`, `share`, `batch`, `tool-output` | Requête utilisée par les compresseurs sensibles à la pertinence (défaut : vide). |
| `--api-key <jeton>` | `serve`, `wrap` | Jeton porteur optionnel envoyé au fournisseur amont. Sa valeur n'est jamais affichée par `--help`, même lue depuis `LM_RESIZER_API_KEY`. Donné sur la ligne de commande, il est lisible par tous les comptes locaux (`ps`, `/proc`) : un avertissement est écrit ; préférer `LM_RESIZER_API_KEY` ou `--api-key-file`. `wrap` ne le transmet jamais en argument au proxy qu'il lance, seulement par l'environnement du processus fils. |
| `--api-key-file <chemin>` | `serve`, `wrap` | Fichier dont la première ligne est le jeton amont. Sous Unix, tout droit du groupe ou des autres entraîne un refus (`mode & 0o077 != 0`) avec la consigne `chmod 600`. Le mode 0600 est conseillé ; 0400 et 0700 passent aussi ce contrôle. L'emporte sur `--api-key`. |
| `--allow-non-loopback` | `serve`, `wrap` | Autorise une adresse `--bind` hors boucle locale. Sans lui, `serve` et `wrap` refusent (le proxy n'authentifie pas ses clients : quiconque l'atteint dépense la clé amont). Par défaut le proxy refuse aussi toute requête dont l'en-tête `Host` n'est pas local (anti rebinding DNS) et ne suit aucune redirection de l'amont. |
| `--exit-code <n>` | `tool-output`, `pipe` | Code de sortie de la commande d'origine (défaut : 0), recopié dans `exit_code` du rapport `--json` ; le processus sort avec ce code, avec ou sans `--json` (`tool-output` sortait 0 avant la 0.2.6). Sous Unix seul l'octet bas d'un code de sortie survit : un code non nul hors de 1 à 255 sort avec son octet bas s'il n'est pas nul, 1 sinon (`256` → 1, `-1` → 255), jamais 0 ; le rapport `--json` garde le code donné. Même règle pour `exec`. |
| `--event <nom>` | `hook` | Nom de l'événement du hook (par exemple `PostToolUse`), recopié dans le rapport `--json`. Défaut : `unknown`. |
| `--ext <liste>` | `batch` | Liste blanche d'extensions séparées par des virgules (`log,json,diff,txt`). |
| `-j`, `--jobs <n>` | `batch` | Nombre de fils de travail (défaut : le pool global de Rayon). |
| `--write-dir <dossier>` | `batch` | Écrit chaque sortie compressée dans ce dossier (un fichier par entrée, même nom). |
| `--clean` | `voice` | Imprime la transcription sans les mots de remplissage au lieu du résumé chiffré. Rejoué avec `um so uh I think` qui devient `so I think`. |
| `--describe` | `image` | Ajoute un résumé visuel déterministe (luminosité, couleur, transparence ; pas d'OCR ni de reconnaissance de scène). |
| `--max-dimension <px>` | `image` | Plafond de la plus grande dimension, appliqué seulement avec `--output`. Minimum 64 : une valeur inférieure est refusée avec ou sans `--output` (`max-dimension must be at least 64`). Ne crée pas un fichier à lui seul. |
| `--output <fichier>` | `image` | Écrit une image plus petite au format de l'entrée (PNG ou JPEG), et seulement si le réencodage est strictement plus petit. Sinon code 0, aucun fichier, pas de ligne `saved` : la ligne d'inspection reste (format, octets, dimensions, conseil, par exemple `small image: safe to keep inline when the model needs visual detail`). L'extension doit suivre le format (`input and output formats must match` ; un PNG vers `.jpg` est refusé). N'écrase pas un fichier déjà présent (`output already exists`). |
| `--quality <1-100>` | `image` | Qualité JPEG à l'écriture avec `--output` (défaut 90). |
| `--max-string <octets>` | `sanitize-provider-fixture` | Remplace par `__LONG_STRING__` toute chaîne d'au moins ce nombre d'octets (défaut 256). |
| `--write` | `learn` | Écrit les recommandations sous `.lm-resizer/learning`. |
| `--install` | `learn` | Installe en plus un bloc réversible dans `AGENTS.md` / `CLAUDE.md` (cible choisie par `--client`). |
| `--full` | `shared-get` | Rend le texte d'origine au lieu de la version compressée. |
| `--advice <json>`, `--advice-from-code-explorer` | `compress` | Conseil structurel (JSON `RetentionAdvice`) sur l'entrée ; ou exige une tentative avec les symboles Code Explorer indexés. Sans conseil valide, compression ordinaire. |
| `--mode <raw\|errors\|tests\|summary>` | `observe` | Politique de sortie (défaut `summary`). |
| `-c`, `--command <texte>` | `run`, `tool-output` | `run` : ligne de commande shell littérale à exécuter. `tool-output` : commande qui a produit le texte fourni (sert à choisir le filtre ; jamais exécutée). |
| `--head-lines <n>` | `read` | Montre seulement les `n` premières lignes du fichier. |
| `-f`, `--filter <nom>` | `pipe` | Nom du filtre appliqué à stdin. |
| `-i`, `--input <fichier>` | `compress`, `expand`, `pipe`, `share`, `voice`, `tool-output` | Fichier d'entrée ; stdin si omis. |
| `--token-budget <n>` | `compress` | Budget de jetons, pour les données structurées seulement (voir `compress --help`). |
| `-r`, `--recursive` | `batch`, `discover`, `eval`, `learn` | Parcourt les dossiers récursivement. |
| `--raw-on-failure` | `exec`, `tool-output` | Sur un code non nul, rend le brut et saute le filtre (`filter` = `raw_on_failure`) : pas de suffixe `:diagnostic-guard`, pas d'étape, pas de message sur stderr. |
| `--stream` | `exec` | Affiche la sortie de l'enfant en direct, puis le résultat filtré après sa sortie. |
| `--list` | `retrieve` | Liste les sorties récupérables. |
| `-H`, `--history`, `-p`, `--project`, `--limit <n>` | `stats` | Dernières exécutions avec comptes et durées ; restreint au dossier courant ; nombre de lignes de `--history` (défaut 20). |
| `--review` | `audit-filters` | Rapport Markdown prêt pour relecture. |
| `--profile <generic\|rust\|node\|python\|infra>` | `init-filters` | Profil de départ du fichier de filtres (défaut `generic`). |
| `--force` | `init-filters`, `init-hooks`, `init-native-hooks`, `init-shims` | Écrase les fichiers existants. |
| `--agent <all\|codex\|claude>` | `discover-sessions` | Magasin de sessions à parcourir (défaut `all`). |
| `--project-dir <dossier>` | `learn`, `init-hooks`, `init-native-hooks`, `init-shims`, `install` | Dossier de projet où écrire la configuration. |
| `--bind <adresse>`, `--dashboard` | `serve` (`--bind` aussi sur `wrap`) | Adresse d'écoute (défaut `127.0.0.1:8787`) ; active le tableau de bord HTML `/dashboard`. |
| `--timeout-sec <n>` | `wrap` | Arrête l'agent lancé après `n` secondes. |
| `--all`, `--file <nom>` | `tee purge` | Supprime tous les fichiers tee, ou un seul. |

`stats` accepte `--json` : c'est la sortie par défaut, le drapeau n'existe que
pour être explicite ; il est incompatible avec `--markdown`.

`retrieve` accepte, sur les trois canaux (CLI, outil MCP `lm_resizer_retrieve`
et `GET /retrieve/<hash>`) : le hash nu, `ccr:<hash>`, `<<ccr:<hash>>>`,
`[full output: <<ccr:<hash>>>]`, `hash=<hash>` et `hash=<hash>]`, ou une **vue
collée** (plusieurs mots ou lignes) qui ne contient qu'une clé distincte — par
exemple une sortie de `compress` qui affiche `hash=`. Une vue sans clé, comme
un journal réduit à un modèle sans `hash=` ni `ccr:`, est refusée
(`CCR entry not found`). Le CLI imprime alors le texte d'origine exact, sans
enveloppe. L'outil MCP et `GET /retrieve/<hash>` renvoient un objet JSON
`{"hash","content"}` : `content` est ce texte, le corps HTTP ne l'est pas
(rejoué : HTTP 200, `content-type: application/json`, `content` de la même
longueur que l'original). Les repères `<<ccr:<12 hex>,<genre>,<taille>>>` et
`<<ccr:<12 hex> N_rows_offloaded>>` d'une vue JSON compressée ne sont **pas**
des clés : ils sont ignorés, la clé est le `hash=` affiché avec la vue. Une
entrée qui contient plusieurs clés distinctes échoue avec
`ambiguous CCR reference` ; un préfixe inconnu (`myhash=`, `notccr:`) ou une
entrée d'un seul mot qui n'est pas l'une des formes ci-dessus est refusé
(`not a recognised CCR key form`). Jamais de choix silencieux entre deux clés.

Codex n'a qu'une configuration utilisateur. `install --client codex --scope project`
est refusé (`Codex MCP config is user-scoped; use --client codex --scope global`) ;
`install --client codex --scope global` écrit `~/.codex/config.toml` ; et
`install --client all --scope project` écrit les fichiers de projet de Claude,
Cursor et VS Code **et** `~/.codex/config.toml` : `--scope project` n'isole pas Codex.

### Options communes à plusieurs commandes

Ces options existent sur plusieurs commandes ; le script de contrôle exige que chaque couple option/commande figure sur une ligne ci-dessous ou dans le tableau précédent.

| Option | Commandes | Effet |
| --- | --- | --- |
| `--json` | `observe`, `err`, `test`, `summary`, `proxy`, `compress`, `batch`, `exec`, `rewrite`, `rewrite-shell`, `image`, `voice`, `ml-status`, `audit-filters`, `verify-filters`, `init-filters`, `sanitize-provider-fixture`, `discover`, `discover-sessions`, `eval`, `learn`, `init-hooks`, `init-native-hooks`, `init-shims`, `install-hooks`, `uninstall-hooks`, `doctor`, `tee list`, `tee purge` | Sortie lisible par machine (JSON) au lieu du texte. |
| `--store` | `pipe`, `compress`, `batch`, `exec`, `tool-output`, `retrieve`, `stats`, `doctor`, `mcp`, `mcp-proxy`, `install`, `serve`, `wrap` | Chemin de la base SQLite CCR (défaut : voir [PROXY-MCP](PROXY-MCP.md)). |
| `--markdown` | `stats`, `discover`, `discover-sessions`, `eval`, `learn` | Sortie en Markdown. |
| `--path` | `trust-filters`, `audit-filters`, `verify-filters`, `init-filters` | Fichier de filtres visé (défaut `.lm-resizer/filters.toml`). |
| `--provider` | `sanitize-provider-fixture`, `serve`, `wrap` | Fournisseur : `openai`, `anthropic`, `bedrock` ou `vertex` (pour `serve` et `wrap`, défaut `openai`). La variable `LM_RESIZER_PROVIDER` n'est lue que par `serve` et `wrap`. |
| `--input` | `sanitize-provider-fixture` | Fichier JSON d'entrée (obligatoire). |
| `--output` | `sanitize-provider-fixture` | Chemin du fichier fixture écrit (obligatoire). |
| `--client` | `init-native-hooks`, `hook`, `install-hooks`, `uninstall-hooks` | Agent visé : `codex`, `claude`, `gemini`, `copilot`, `cursor` ou `all` (défaut `all`). `uninstall-hooks --client all` retire les configurations natives des cinq clients. Pour `install` et `uninstall` (serveur MCP) : `claude`, `codex`, `cursor`, `vscode` ou `all`, avec `--scope project|global` (Codex toujours global) ; `uninstall --scope all` retire les deux portées. `uninstall-hooks` reconnaît un fichier généré pour le binaire courant ou pour le chemin de binaire écrit dans le fichier. `install-hooks` n'accepte que `codex`, `claude` et `all` ; pour `gemini`, `copilot` et `cursor`, utiliser `init-native-hooks`. |
| `--project-dir` | `install-hooks`, `uninstall-hooks`, `install`, `uninstall` | Dossier du projet (où lire ou écrire `AGENTS.md` / `CLAUDE.md` ou la configuration). |
| `--force` | `install-hooks` | Écrase les fichiers générés existants. |

## Garde de diagnostic

Les filtres de commande gardent les lignes qui comptent ; l'étape générique qui
suit, elle, ne connaît pas la commande et range les lignes par fréquence. Deux
pertes distinctes sont surveillées, par deux mécanismes distincts. Le suffixe
signale une perte du filtre, l'étape une perte de l'étape générique. L'un
n'entraîne pas l'autre ; les deux peuvent pourtant être présents ensemble.

- **Le filtre lui-même a laissé tomber une ligne d'échec** — par exemple une
  ligne `exit code 2`, `##[error]`, `error`, `panic` ou une localisation
  `fichier:ligne` : le brut est rendu tel quel et `filter` reçoit le suffixe
  `:diagnostic-guard`. Le chemin de capture par défaut d'`exec` préfixe en plus
  un nom non natif par `native:` (rejoué : `native:toml:make:diagnostic-guard`,
  étapes vides, stderr vide). `exec --stream` et `tool-output` ne préfixent
  pas `native:`. `hook` hors `PreToolUse` / `BeforeTool` applique le même filtre : rejoué,
  `hook --event PostToolUse` enregistre `toml:make:diagnostic-guard`. `pipe` et
  `discover` appellent aussi ce filtre. `exec` et `tool-output` ne sont donc
  pas les seules commandes à le faire. `PreToolUse` réécrit la commande, il ne
  pose pas ce suffixe.
- **Le suffixe n'est pas posé sur tous les chemins.** `exec --raw-on-failure`
  et `tool-output --raw-on-failure`, sur un code non nul, ne lancent pas le
  filtre. Rejoué sur `exec` : `filter` vaut `raw_on_failure`, étapes vides,
  stderr vide, brut rendu, `tee_hint` null. Ni suffixe, ni message.
- **L'étape générique suivante omettrait une ligne que le filtre avait gardée** :
  le corps filtré est conservé — la réduction déjà faite par le filtre n'est
  pas annulée — et `compression_steps` contient `diagnostic_gate:kept_filtered`.
  Seules les étapes génériques sont annulées. `exec --stream` écrit alors sur
  stderr `lm-resizer: compression générique annulée, elle omettait « … »` ;
  `tool-output` reste silencieux. Les deux marqueurs peuvent être présents
  ensemble, sans réduction : rejoué, `exec --stream` sur le `make` de 4 835
  octets (le filtre avait perdu la ligne) donne `toml:make:diagnostic-guard`,
  étapes `log_offload` et `diagnostic_gate:kept_filtered`, 4 835 → 4 835, et
  ce message sur stderr. La vue imprimée peut ensuite gagner une ligne
  `[tee:<id>]` quand la réduction paie cette remorque ; la remorque n'est pas
  l'étape. Rejoué, `tool-output --command 'make build'` dont le filtre garde
  `error:` et écarte le bruit : 6 027 octets, corps filtré 27, vue imprimée 46
  avec `[tee:]`, filtre `toml:make` sans suffixe, étapes vides, stderr vide.
- **Chemin de capture par défaut d'`exec`** (ni `--stream` ni
  `--raw-on-failure`) : l'étape générique n'est lancée que pour le filtre
  `native:native_owned` (un appel imbriqué à `lm-resizer`). Ce n'est pas le
  résultat du filtre rendu tel quel. Ce chemin réinjecte comme `compress`
  (`diagnostic_reinjection:N` dans `compression_steps`) et garde la vue
  comprimée si plus aucune ligne d'échec ne manque. Rejoué : 4 835 octets,
  filtre `native:native_owned`, étapes `log_offload` et
  `diagnostic_reinjection:1`, sortie 242 octets, la ligne `exit code 2` est
  présente, stderr vide. Pour tout autre filtre, ce chemin rend ce que le
  filtre a produit, sans étape générique.

`compress` ne refuse pas la réduction : il **réinjecte** les lignes d'échec
omises à la suite du texte, dans leur ordre d'origine, sous le marqueur
`[lm-resizer: N lignes d'échec omises par la compression, réinjectées ci-dessous]`
(`steps_applied` contient `diagnostic_reinjection:N`). Si le résultat n'est
pas plus petit que l'entrée, le texte d'origine est rendu inchangé. Coller
dans `retrieve` une sortie de `compress` qui n'a ni `hash=` ni `ccr:` (un
journal `INFO` réduit à un modèle) échoue : ce n'est pas une clé.

C'est pourquoi une sortie qui contient un diagnostic peut n'afficher presque
aucun gain : la réduction générique est sacrifiée pour que l'erreur reste sous
les yeux. Le code de cette garde n'a pas changé en 0.2.4 ; cette section ne
fait que le décrire.

## Variables d'environnement

| Variable | Effet |
| --- | --- |
| `LM_RESIZER_UPSTREAM` | URL de base du fournisseur compatible OpenAI, valeur par défaut de `--upstream` pour `serve` et `wrap`. |
| `LM_RESIZER_API_KEY` | Valeur par défaut de `--api-key` (jamais affichée). |
| `LM_RESIZER_API_KEY_FILE` | Valeur par défaut de `--api-key-file`. |
| `LM_RESIZER_PROVIDER` | Valeur par défaut de `--provider` pour `serve` et `wrap` seulement : `openai`, `anthropic`, `bedrock` ou `vertex`. Une autre valeur est refusée quand ces commandes lisent leurs arguments (`unsupported provider 'bogus'. Use openai, anthropic, bedrock, or vertex`, code 1). Ce n'est pas un refus au démarrage de toute commande : `stats` et `doctor` avec `bogus` sortent 0, stderr vide. |
| `LM_RESIZER_CODE_EXPLORER_BIN` | Binaire Code Explorer utilisé par `compress --advice-from-code-explorer` (d'après son `--help`) ; à défaut, `code-explorer` dans le `PATH`. |
| `LM_RESIZER_TEE` | `0` coupe l'archive du brut de `exec` (`[raw: …]`, `tee_hint`) ; le hash CCR de `compress` reste émis. |
| `LM_RESIZER_STORE`, `LM_RESIZER_STATE_DIR` | Chemin du store CCR et dossier d'état ; voir [PROXY-MCP](PROXY-MCP.md). |

## Ce qui n'a pas été rejoué de bout en bout

- `--api-key`, `LM_RESIZER_UPSTREAM`, `LM_RESIZER_API_KEY` : le texte vient de
  `--help` ; aucun fournisseur amont réel n'a été appelé pour cette page.
- `LM_RESIZER_CODE_EXPLORER_BIN` : lu par l'aide de `compress` ; aucun index Code
  Explorer n'était disponible, donc le comportement avec un binaire valide n'a
  pas été observé.
- `smart` : rejoué sur un petit fichier Rust sans index ; le chemin avec symboles
  indexés n'a pas été observé.
- `learn --write` et `--install` : lus dans `--help`, non exécutés.

## Generic views (`err`, `test`, `summary`) and `gain`

`lm-resizer err -- <program> [args...]`, `test`, and `summary` run any program. They capture the producer's output and preserve its exit status. `err` keeps diagnostic lines, `test` keeps diagnostics and test totals, and `summary` also retains numeric footer lines. Selected lines keep adjacent context; traceback frames, source lines and compiler carets stay with their diagnostic. On failure the first line includes the real exit code, including when raw output is requested. Unknown diagnostics fall back to the complete output. The full output is archived; use `lm-resizer tee list` and `lm-resizer tee read <id>` to recover it. Add `--json` before `--` for the output, status, exact token counts and recovery hint.

`lm-resizer exec -- <program> [args...]` applies a dedicated native view where one exists. Any other program, script, wrapper (`env`, `timeout`, `xargs`…), shell line or recipe runner (`make`, `npm run`, `npm test`, `cargo run`, `uv run`…) is returned raw, byte for byte, apart from the `[FAIL]` header on a nonzero code; only a whole JSON document or a run of log-level lines is folded, reversibly. Since 0.2.6 the generic summary is never applied by itself: ask for it with `summary`. A test runner's view is raw (`lossless:test-output`) when the command asks to display test output (`cargo test -- --nocapture`, `pytest -s`, `go test -v`, `jest --silent=false`, `dotnet test -v d`…) or when the runner has no capture (`rspec`, minitest, Maven test phases, `playwright test`); a test runner that exits with code 0 is returned raw too (`lossless:test-success`), its reduced view applying only to a nonzero code; recipe runners are `lossless:script-runner`, other unknown programs `lossless:generic`. `git log` is shortened only in a simple direct call (see [KNOWN-MISSES](KNOWN-MISSES.md)). `lm-resizer tool-output --command '<program> [args...]' --exit-code N` accepts already captured text on stdin and follows the same rules. Both write measured execution history. Hooks that route through `exec` and the MCP `lm_resizer_tool_output` tool use the same history. MCP applies the same views by default and marks nonzero producer status with `isError: true`; `raw_on_failure: true` keeps the raw body while still showing the failure code first.

`lm-resizer gain` displays the total number of commands, original and output tokens, signed savings and percent saved. `lm-resizer gain --json` and `lm-resizer stats --json` expose the underlying counts in `exec_history`; `stats` also defaults to JSON. `--history` prints recent executions and `--project` restricts them to the current directory, whose path is displayed even for a zero count. Token counts use tiktoken-rs `o200k_base` on the final output including visible recovery hints. Legacy records lacking exact counts remain separate as estimates and do not inflate the measured total.
