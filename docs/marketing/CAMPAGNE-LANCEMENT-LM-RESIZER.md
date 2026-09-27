---
title: "Campagne de lancement : LM Resizer (lm-resizer)"
author: "Patrice Huetz"
date: "27 septembre 2026"
lang: fr-FR
---

# Campagne de lancement : LM Resizer

> Document de travail du dimanche 27 septembre 2026 (~22 h, heure de Paris). Base : dépôt `phuetz/lm-resizer`, branche `master` au commit `57c66e5` (24/09/2026, fusion de la PR #20), release `v0.2.2`. Le binaire a été **compilé depuis ce commit et testé sur une machine Linux** (rustc 1.98.1) : chaque commande citée a été vérifiée avec `--help`. **`[à vérifier]`** = point non confirmé par le code ou dépendant de votre environnement. **Rien n'a été publié** (ni X, ni LinkedIn, ni Reddit, ni HN, ni npm).
>
> Règle de vocabulaire de ce document : on écrit **« Apache-2.0 »**, **« licence permissive »** / **« permissive license »**. Le terme anglais courant pour qualifier les licences de ce type est volontairement proscrit dans tous les textes (FR et EN).

## 1. Points d'attention avant publication

### 1.1 Bloquants

1. **Licence : PR #14 ouverte (Business Source License 1.1).** `master` est en **Apache-2.0** (`LICENSE`, `Cargo.toml`, `plugin.json`, `marketplace.json`). La PR #14 (`chore/licence-busl-2026-09-18`) passerait le projet en BSL 1.1, comme Code Explorer. **Si elle est fusionnée, « Apache-2.0 », « licence permissive » et « permissive license » doivent être remplacés PARTOUT dans cette campagne** (tweets 6/6 et accroche B, LinkedIn, Reddit, HN, carte de fin vidéo) par « BSL 1.1 (source-available) », et la présentation sur r/rust doit être revue. Décision à prendre **avant le gel du 2/11**. Changer de licence pendant ou juste après le Show HN est le pire scénario.
2. **Chiffre du hero README : à vérifier / non reproductible.** Le README dit : « One `cargo test` run through `lm-resizer`: **398 commands, 1.23 MB → 372 KB, 222,247 tokens saved** ». Ce que le code mesure réellement :
   - `398 commands` est le **cumul de l'historique `exec`** (`exec_history.commands` dans `lm-resizer stats`), pas le nombre de commandes d'un seul `cargo test`. Un `cargo test` = 1 commande.
   - Le code calcule `estimated_tokens_saved = bytes_saved / 4` (`src/main.rs`, l. 4039 et 4134). Avec 1,23 Mo → 372 Ko (1 231 792 → 372 040 octets), cela donne **214 938**, pas **222 247**. L'écart (~3 %) n'est pas expliqué `[à vérifier : autre capture, autre version, ou arrondi des octets]`.
   - La sortie réelle de `lm-resizer stats` est du JSON (ou Markdown avec `--markdown`), pas le format de l'image hero.
   - **Formulation retenue dans cette campagne** (chiffres imposés, dits exactement) : « sur une session de 398 commandes passées par lm-resizer (cumul, pas un seul `cargo test`) : 1,23 Mo → 372 Ko, 222 247 tokens estimés économisés ». **Ne jamais écrire qu'un seul `cargo test` a produit 222 247 tokens.** Idéalement, refaire une capture datée de `lm-resizer stats --markdown` avant J-1 et remplacer les trois chiffres par ceux de la capture.
3. **Le README et les manifestes documentent des fonctions absentes du binaire `master`** (vérifié sur le binaire compilé depuis `57c66e5`) :
   - `lm-resizer tool-output` → `error: unrecognized subcommand 'tool-output'` ;
   - `lm-resizer exec --token-budget` → refusé (seul `compress` accepte `--token-budget`) ;
   - route HTTP `POST /tool-output` et outil MCP `lm_resizer_tool_output` → absents (`tools/list` du serveur MCP = `lm_resizer_compress`, `lm_resizer_retrieve`, `lm_resizer_stats`, rien d'autre) ;
   - mentions concernées : `README.md` l. 183, 231–245, 400, 451, 538 ; `skills/lm-resizer/SKILL.md` l. 61 ; `.claude-plugin/plugin.json` (« compress, tool_output, retrieve, stats ») ; `docs/marketing/VIDEO-LISA-2026-09-lm-resizer.md` l. 216–225, 264–265, 300.
   - → Livrer la branche qui contient `tool-output` **ou** retirer ces lignes avant le lancement. Cette campagne **ne cite pas** `tool-output`.
4. **Une sortie peut grossir sur `master`.** La PR #18 (ouverte, `fix/porte-non-croissance-exec-2026-09-18`) mesure sur 32 483 commandes réelles : 303 Mo → 123 Mo (−59,5 %), mais **12,95 % des sorties ont grossi** (pires cas `rg` : +39 % à +54 %) ; la porte de non-croissance n'existe que dans le proxy. **Fusionner la PR #18 avant J-7**, puis refaire la mesure : un commentateur HN le testera.
5. **`docs/marketing/LINKEDIN-2026-09-lm-resizer.md` contient le terme proscrit.** Ligne 31 : « lm-resizer, notre outil **[o•••• s•••••]** en Rust, filtre la sortie des commandes avant qu'elle n'atteigne l'agent de code : un `cargo test` complet passe de 1,23 Mo à 372 Ko, 222 247 tokens économisés, sans perdre une erreur ni un chemin de fichier. » (terme masqué ici pour respecter la règle ; c'est l'expression anglaise en deux mots « o… s… »). Ligne 39 : hashtag `#O…Source`. → Remplacer par « notre outil en Rust, sous licence Apache-2.0 » et supprimer le hashtag. La même ligne 31 (et les l. 13, 47) attribue les 398 commandes / 222 247 tokens à **un seul** `cargo test` : à corriger selon le point 2. Même problème dans `docs/marketing/VIDEO-LISA-2026-09-lm-resizer.md` (l. 24, 56, 440, 489 : « 222 247 tokens d'un test ») et `docs/marketing/LINKEDIN-2026-09-skills-agents.md` l. 5.

### 1.2 À corriger (non bloquant mais visible)

6. **Attribution.** Le README crédite RTK (« RTK-inspired ») mais `CONTRIBUTING.md` parle d'un « port », et le code reprend le vocabulaire de **Headroom** (CCR, SmartCrusher, « live zone ») sans mention dans le README. Ajouter une section « Acknowledgements / Prior art » (RTK, Headroom) `[à vérifier : si du code a été porté, Apache-2.0 impose de conserver les notices / fichier NOTICE]`. Sur HN, « port of what? » arrivera tôt.
7. **Binaires.** Seule la v0.2.0 a un binaire publié (`lm-resizer-0.2.0-linux-x86_64.tar.gz`). v0.2.1 et v0.2.2 : aucun artefact. Pas de crate sur crates.io. Installation = `cargo install --locked --git …` (1 min 24 sur la machine de test ; **exige Rust ≥ 1.86 et un compilateur C++**, alors que `Cargo.toml` annonce `rust-version = "1.80"`).
8. **Versions désalignées.** Dépôt/tag 0.2.2 ; npm `latest` = 0.2.1 ; `plugin.json` et `marketplace.json` annoncent 0.2.1. Pas de `--version` sur la CLI.
9. **npm.** Publication par trusted publishing **OIDC** avec provenance depuis 0.2.1 (workflow `publish-wasm.yml`). Vérifier sur npmjs.com que le trusted publisher pointe toujours sur `phuetz/lm-resizer` / `publish-wasm.yml` avant la release du 10/11 ; le secret `NPM_TOKEN` n'est qu'un secours (envisager de le supprimer). `packages/wasm/package.json` n'a pas de champ `license`.
10. **`rewrite-shell` et pipes** `[à vérifier]` : le README dit qu'un segment avec pipe est laissé intact, un test antérieur a montré le contraire.

### 1.3 Faits vérifiés (utilisables tels quels)

| Affirmation | Statut | Preuve |
|---|---|---|
| `exec -- cargo test`, `exec --stream`, `exec --raw-on-failure`, `exec --json`, `exec -q` | ✅ existe | `lm-resizer exec --help` |
| `mcp` (serveur stdio, 3 outils : compress, retrieve, stats) | ✅ | `mcp --help` + `tools/list` |
| `install --client claude\|codex\|cursor\|vscode\|all --scope project\|global` | ✅ | `install --help` |
| `init-native-hooks --client codex\|claude\|all` | ✅ | `init-native-hooks --help` |
| `init-shims` (shims PATH opt-in) | ✅ | `init-shims --help` |
| `discover <paths>` et `discover-sessions` | ✅ | `--help` |
| `learn <paths> [--write] [--install]` | ✅ | `learn --help` |
| `stats [--markdown]` | ✅ | `stats --help` |
| `retrieve <hash>` (store SQLite CCR) | ✅ testé : `compress` d'un JSON de 23 891 octets → clé `e39821d8…`, `retrieve` rend l'original (22 091 octets, JSON minifié) | test du 27/09 |
| `serve --dashboard` (page `/dashboard`), `--upstream`, `--provider openai\|anthropic\|bedrock\|vertex` | ✅ | `serve --help` |
| `rewrite -- cargo test` → `lm-resizer exec -- cargo test` (n'exécute rien) | ✅ testé | |
| `trust-filters [--path]` (+ `verify-filters`, `audit-filters`, `init-filters`) | ✅ | `--help` |
| `claude plugin marketplace add phuetz/lm-resizer` | ✅ manifeste présent : `.claude-plugin/marketplace.json` (marketplace `phuetz-tools`, plugin `lm-resizer`, source `./`) + `plugin.json` (skills) + `.mcp.json` (serveur `lm-resizer mcp`). Installation réelle dans Claude Code `[à vérifier : non testée ici]` | fichiers du dépôt |
| `tool-output`, `exec --token-budget`, `POST /tool-output`, `lm_resizer_tool_output` | ❌ **absents** du binaire `master` | voir 1.1 point 3 |
| Magika (ONNX) optionnel, désactivé par défaut | ✅ feature Cargo `magika` hors `default = []` **et** `LM_RESIZER_ENABLE_MAGIKA=1` ; `ml-status` sur le binaire standard : « Magika enabled: false … ONNX runtime: not compiled in » | `Cargo.toml`, `ml-status` |
| Tokens = estimation `octets / 4` | ✅ | `src/main.rs` |
| WASM ~2,6 Mo, Node ≥ 18 | ✅ `packages/wasm/README.md` (« about 2.6 MB ») ; npm `dist.unpackedSize` = 2 748 762 octets (paquet complet) ; `engines.node >=18` | npm, dépôt |
| Smoke npm 3 161 → 1 222 octets | ✅ écrit dans `packages/wasm/README.md` l. 59 ; non rejoué ici `[à vérifier : node packages/wasm/smoke.mjs après build]` | |
| Validation live 2026-06-23 : Mistral, Ollama, DeepSeek, OpenRouter, xAI | ✅ écrit dans le README (l. 42–45 ; Mistral : ~5,8 Ko → 2,8 Ko) ; non rejoué ici | README |
| Démo réelle (crate de 300 tests + 1 échec) | ✅ `cargo test` brut 323 lignes / 9 026 octets → `exec` 19 lignes / 660 octets (−92,7 %), code de sortie 101 conservé ; tout vert : 314 → 4 lignes | test du 27/09 |

## 2. Positionnement

**Phrase courte** : lm-resizer est une **couche de compression de contexte native Rust** pour Claude Code, Codex et les agents MCP : il filtre et compresse la sortie des commandes (tests, diffs, logs, `rg`, JSON, trafic fournisseur) avant qu'elle n'atteigne le modèle, et garde l'original récupérable en local.

**Dans la chaîne d'outils** (même auteur) :

| Outil | Rôle | Question à laquelle il répond |
|---|---|---|
| **Code Explorer** | la **carte** : graphe de connaissance du dépôt (symboles, appelants, impact) | « Où regarder ? » |
| **lm-resizer** | le **filtre** : ce que l'agent lit après avoir lancé une commande | « Qu'est-ce qui mérite d'entrer dans le contexte ? » |
| **Code Buddy** | l'**orchestration** du travail de l'agent | « Qui fait quoi, dans quel ordre ? » |

**Modes (tous vérifiés sur `master` sauf mention)** :

- **CLI** : `lm-resizer exec -- <cmd>` (filtre + compression, code de sortie conservé), `exec --stream` (sortie live puis résultat filtré), `compress`, `batch`.
- **Hooks natifs Claude Code / Codex** : `init-native-hooks` (réécriture des commandes Bash connues ; commande inconnue = exécutée brute ; le hook ne bloque jamais).
- **Serveur MCP** : `lm-resizer mcp`, installé par `lm-resizer install --client claude|codex|…`.
- **Plugin Claude Code + skills** : `claude plugin marketplace add phuetz/lm-resizer` puis `claude plugin install lm-resizer@phuetz-tools` `[à vérifier : installation réelle]` ; skills dans `skills/`.
- **Shims PATH** : `init-shims` (opt-in, route automatiquement les commandes connues via `exec`).
- **Proxy HTTP** : `lm-resizer serve` devant une API compatible OpenAI/Anthropic (+ Bedrock, Vertex), tableau de bord local `--dashboard`.
- **Récupération CCR** : blocs déchargés dans un store **SQLite local**, relus par `lm-resizer retrieve <hash>` ; sortie brute des commandes via `lm-resizer tee`.
- **Compression orientée requête** : `-q "<question>"` ; s'il faut couper, on garde en priorité ce qui touche la question (`--token-budget` sur `compress` uniquement).
- **Détection Magika (ONNX) optionnelle** : désactivée par défaut, activable à la compilation (`--features magika`) et à l'exécution (`LM_RESIZER_ENABLE_MAGIKA=1`).
- **Module WASM npm** (~2,6 Mo, Node ≥ 18) : compression JSON seulement, pas de filtrage de logs.

**Ce qu'on ne dit pas** : « meilleur que RTK / Headroom » (ce sont les références, bien plus mûres) ; « X % d'économie » sans périmètre ; « aucune perte » sans préciser que l'original est récupérable localement. **Ton** : concret, chiffres datés et reproductibles.

**Coexistence** : si RTK est installé, lm-resizer reconnaît `rtk …` et ne refiltre pas sa sortie (filtre `rtk_owned` testé dans le code).

## 3. Script vidéo (40 s, version 60 s en option)

**Principe** : écran partagé, sorties réelles uniquement. À gauche le `cargo test` brut, à droite la même commande via `lm-resizer exec`. Puis la preuve que rien n'est perdu. Voix off FR + sous-titres incrustés (lisible en muet).

**Préparation** : crate de démo (300 tests + 1 échec) ou vrai projet public avec un test cassé volontairement ; `export LM_RESIZER_STATE_DIR=~/demo-state` (état vierge) ; `tmux` 2 volets, `PS1='$ '`, police 20–22 px ; VHS pour les plans terminal, OBS pour Claude Code.

| Temps | Scène / écran | Commande à l'écran | Voix off (FR) | Texte incrusté |
|---|---|---|---|---|
| 0:00–0:04 | Split figé : mur de `test … ok` à gauche, 19 lignes nettes à droite, compteurs 323 / 19 | — | « Un cargo test : trois cent vingt-trois lignes. » | **323 lignes → 19. Même échec.** |
| 0:04–0:10 | Volet gauche : `cargo test` défile (accéléré, afficher « ×4 ») | `cargo test` | « Votre agent de code les lit toutes. Et les paie. » | Ce que l'agent lit d'habitude |
| 0:10–0:17 | Volet droit : frappe de la commande ; apparaissent `FAILED`, `left: 4 / right: 5`, `300 passed; 1 failed` | `lm-resizer exec -- cargo test` | « Avec lm-resizer, il ne voit que le test en échec, l'assertion et le résumé. » | Le signal. Le reste est filtré. |
| 0:17–0:20 | `echo $?` → `101` | `echo $?` | « Le code de sortie est conservé. » | Code de sortie : 101 |
| 0:20–0:26 | Zoom sur `[full output: …]`, puis relecture du brut | `lm-resizer tee list` puis `lm-resizer tee read <fichier>` | « Et la sortie complète reste sur votre disque. » | Rien n'est perdu, tout reste local |
| 0:26–0:33 | Session Claude Code réelle : l'agent lance `cargo test`, le hook le réécrit `[à vérifier : filmer une vraie session]` | `lm-resizer init-native-hooks --client claude` | « En hook dans Claude Code et Codex, c'est automatique. » | Hooks Claude Code / Codex |
| 0:33–0:37 | JSON de la run | `lm-resizer exec --json -- cargo test` (`original_bytes: 9026`, `compressed_bytes: 660`) | « Neuf mille octets, six cent soixante. » | 9 026 → 660 octets (cette démo) |
| 0:37–0:40 | Carte de fin | — | « lm-resizer. Rust, Apache-2.0, cent pour cent local. » | voir ci-dessous |

**Version 60 s** (insérer après 0:33) : 8 s `lm-resizer install --client claude` + `lm-resizer mcp` (outils listés : compress, retrieve, stats) ; 7 s `lm-resizer serve --dashboard` + page `http://127.0.0.1:8787/dashboard` ; 5 s `lm-resizer stats --markdown` (flouter chemins). Voix : « Aussi en serveur MCP, et en proxy local devant votre API, avec un tableau de bord. »

**Carte de fin (3 s)** :

```
lm-resizer : moins de bruit, plus de contexte utile
cargo install --locked --git https://github.com/phuetz/lm-resizer
github.com/phuetz/lm-resizer
Apache-2.0 · 100 % local · Claude Code · Codex · MCP
```

Honnêteté : afficher « ×4 » sur les accélérations ; mention « démo : crate de test, 300 tests + 1 échec » ; **ne pas** présenter 222 247 tokens comme résultat de cette run. Formats : 16:9 1920×1080 (X, HN, LinkedIn, README), 9:16 1080×1920, GIF ≤ 8 Mo pour le README ; `.srt` FR + EN. Si PR #14 fusionnée : remplacer « Apache-2.0 » par « BSL 1.1 ».

## 4. Thread X (français)

Comptage X (twitter-text v3) : URL = 23, emoji = 2 ; les caractères `•`, `→`, `…` comptent 2 et ne sont pas utilisés. Le nom du paquet npm scopé n'est pas écrit (X le transformerait en mention). Script : `tweets.py` / `tweets.json` (copie box `/workspace/lm-resizer-launch/v2/`).

**Tweet 1/6** (257) — avec la vidéo 16:9 en upload natif

```text
Un cargo test : 323 lignes de sortie.
Ce que mon agent de code lit avec lm-resizer : 19 lignes. Le test en échec, l'assertion, le compteur final.
La sortie brute reste sur le disque, récupérable.

Un binaire Rust pour Claude Code, Codex et les agents MCP 🧵
```

**Tweet 2/6** (269)

```text
Le principe : lm-resizer se place entre la commande et le modèle.

lm-resizer exec -- cargo test

Filtres par famille (git, cargo, rg, vitest/jest, Terraform, kubectl...), puis compression. Code de sortie conservé.
Avec --stream, vous voyez défiler la sortie en direct.
```

**Tweet 3/6** (254)

```text
Sans changer vos habitudes :

lm-resizer init-native-hooks --client all

Claude Code / Codex exécutent la version filtrée des commandes Bash connues. Commande inconnue = sortie brute, jamais bloquée.
Ou en serveur MCP : lm-resizer install --client claude
```

**Tweet 4/6** (255)

```text
Au-delà du terminal :
- proxy local devant une API compatible OpenAI/Anthropic (lm-resizer serve)
- compression orientée requête : s'il faut couper, on garde ce qui touche votre question
- blocs retirés stockés en SQLite local : lm-resizer retrieve <hash>
```

**Tweet 5/6** (273) — chiffres à revalider à J-1 (voir 1.1 point 2)

```text
Les chiffres, dits précisément.
Démo ci-dessus : 9 026 -> 660 octets.
Cumul d'une session de 398 commandes passées par lm-resizer (pas un seul cargo test) : 1,23 Mo -> 372 Ko, 222 247 tokens estimés économisés.

L'idée n'est pas neuve : RTK et Headroom sont les références.
```

**Tweet 6/6** (252)

```text
Installer (Rust stable récent + compilateur C++) :
cargo install --locked --git https://github.com/phuetz/lm-resizer

Apache-2.0, 100 % local, pas de télémétrie.
Repo 👉 https://github.com/phuetz/lm-resizer

Projet jeune : dites-moi quelle commande inonde encore votre contexte.
```

**Accroche alternative A** (208)

```text
Votre agent de code paie des tokens pour lire "test ... ok" 300 fois.

lm-resizer ne lui montre que ce qui compte : l'échec, l'assertion, le résumé. Le brut reste récupérable en local.

Démo réelle en 40 s 👇
```

**Accroche alternative B** (229)

```text
Une fenêtre de 1M de tokens ne rend pas le bruit gratuit : vous le payez en coût, en latence et en attention du modèle.

lm-resizer filtre la sortie des commandes avant qu'elle n'atteigne Claude Code ou Codex. Rust, Apache-2.0 👇
```

Conditions : tweet 5 → si la capture refaite donne d'autres chiffres, les remplacer (ou écrire « environ 215 000 tokens estimés (octets / 4) », valeur cohérente avec le code) ; tweet 6 et accroche B → « Apache-2.0 » seulement si la PR #14 n'est pas fusionnée ; si un binaire v0.3.0 est publié le 10/11, remplacer la ligne `cargo install` par le lien de release. Épingler le thread 7 jours ; répondre au tweet 1 avec le lien du fil HN si le Show HN prend.

## 5. Post LinkedIn (français)

> Un `cargo test`, 323 lignes. Mon agent de code n'a besoin que de 19 d'entre elles.
>
> C'est le problème que traite lm-resizer, l'outil que je développe en Rust : Claude Code et Codex dépensent une grosse part de leur fenêtre de contexte à lire des sorties de commandes (tests qui passent, logs d'installation, diffs, recherches). Ça coûte des tokens, ça ralentit, et ça noie l'erreur qui compte.
>
> lm-resizer se place entre la commande et le modèle :
> - il garde le test en échec, l'assertion, les chemins et le résumé ;
> - il conserve le code de sortie et range la sortie brute en local, récupérable ;
> - il s'installe en hook dans Claude Code et Codex, en serveur MCP, ou en proxy devant une API compatible OpenAI/Anthropic.
>
> Les chiffres, avec leur périmètre : sur une session de 398 commandes passées par lm-resizer, 1,23 Mo de sorties sont devenus 372 Ko, soit 222 247 tokens estimés économisés. Sur la démo ci-dessous, un seul `cargo test` passe de 9 026 à 660 octets.
>
> Il complète les deux autres outils que j'utilise chaque jour : Code Explorer, la carte du dépôt, et Code Buddy, qui orchestre le travail de l'agent.
>
> Limites : projet jeune, installation par compilation Rust pour l'instant. Licence Apache-2.0, 100 % local, sans télémétrie.
>
> Démo de 40 s ci-dessous. Le lien du dépôt est en commentaire.
>
> Vous mesurez ce que vos agents lisent vraiment ?
>
> #IA #DevTools #ClaudeCode #Rust #LLM #AgentsIA

Premier commentaire : `https://github.com/phuetz/lm-resizer` + « Pour l'installer dans Claude Code : `claude plugin marketplace add phuetz/lm-resizer` ». Vidéo native 1:1 ou 9:16. Chiffres 398 / 222 247 : voir 1.1 point 2 (à revalider).

## 6. Reddit et Hacker News (anglais)

Règles générales : disclosure « I'm the author » en tête ; écrire avec vos mots (les modérations r/ClaudeAI, r/rust, r/LocalLLaMA filtrent les textes générés) ; relire la barre latérale le jour même `[à vérifier : règles au jour J]` ; répondre pendant 2 h.

### 6.1 r/ClaudeAI (mer. 18/11, 15 h 30 Paris)

Rappels : règle « Showcase » (expliquer ce qui a été construit, comment Claude a aidé, gratuit à essayer) ; karma ≥ 50 `[à vérifier]` ; flair obligatoire.

**Title:** `I built a Rust PreToolUse hook that keeps Claude Code's failing tests and drops the "... ok" noise (323-line cargo test -> 19 lines, raw output kept locally)`

> I'm the author. lm-resizer is a local Rust binary that sits between the commands Claude Code runs and the model. It keeps what the model needs (failing test names, assertion diffs, file paths, final counters) and drops repetitive noise. The raw output is saved locally, so nothing is lost.
>
> ```
> cargo install --locked --git https://github.com/phuetz/lm-resizer   # recent stable Rust + a C++ compiler
> lm-resizer exec -- cargo test                 # exit code preserved
> lm-resizer init-native-hooks --client claude  # rewrites supported Bash commands automatically
> # or as a plugin (skill + MCP server):
> claude plugin marketplace add phuetz/lm-resizer
> claude plugin install lm-resizer@phuetz-tools
> ```
>
> **Numbers, with their scope.** Demo crate (300 passing + 1 failing test): 323 lines / 9,026 bytes -> 19 lines / 660 bytes, exit code 101 kept, failing assertion fully visible. Over a session of 398 commands routed through lm-resizer (cumulative, not one run): 1.23 MB -> 372 KB. Tokens are estimated as bytes / 4.
>
> **How the hook behaves:** it only rewrites commands it has a filter for (git, cargo, rg, vitest/jest, Terraform, kubectl, Go, .NET, JVM, Playwright, RSpec, Prisma...). Unknown commands run raw; the hook never blocks. `lm-resizer rewrite -- <cmd>` shows what would happen without running anything.
>
> **Getting the full output back:** `lm-resizer tee read <file>` for raw command output; compressed blocks live in a local SQLite store and come back with `lm-resizer retrieve <hash>` (also exposed as an MCP tool).
>
> **How Claude was used:** a large part of the code was written with Claude Code; I review, measure and run the test suite myself `[à adapter honnêtement]`.
>
> **Prior art:** RTK and Headroom are the established projects here, and much more mature. lm-resizer puts command filters, an MCP server, a local proxy and recovery in one Rust binary.
>
> **Limits:** young project, one maintainer; no prebuilt binary for the latest version yet; not on crates.io.
>
> Free, Apache-2.0 (permissive license), local, no telemetry: https://github.com/phuetz/lm-resizer
>
> Which commands still flood your Claude Code context?

### 6.2 r/rust (ven. 20/11, 15 h 00 Paris)

Rappels : au plus 1 auto-promo par semaine, contenu centré Rust, méfiance envers les « showcase » peu travaillés → retour d'expérience technique.

**Title:** `Lessons from building a context-compression layer for coding agents as a single Rust binary (clap CLI, axum proxy, SQLite recovery store, WASM build)`

> I'm the author of lm-resizer, a Rust tool that filters and compresses command output before it reaches coding agents (Claude Code, Codex, MCP clients). A few things I learned building it that may interest this sub:
>
> - **One binary, several surfaces.** Same pipeline behind `exec` (command wrapper), a stdio MCP server, native agent hooks, opt-in PATH shims and a local HTTP proxy (`serve`, OpenAI/Anthropic/Bedrock/Vertex request shapes). Keeping the pipeline a library crate (`lm-resizer-core`) made the WASM build (`wasm32-unknown-unknown`, ~2.6 MB, JSON only) mostly free.
> - **Recovery over trust.** Anything dropped is stored in a local SQLite store keyed by hash (`lm-resizer retrieve <hash>`). It makes aggressive filtering acceptable.
> - **Heavy deps stay optional.** ML content detection (Google's `magika` crate, ONNX) is behind a Cargo feature and a runtime env var, off by default.
> - **Filters can make output bigger.** Measuring my own history showed some `rg` results grew after annotation; a "never grow" gate is being extended to every path `[à mettre à jour selon l'état de la PR #18]`.
> - **Pain points:** MSRV drift (transitive `icu_*` now needs 1.86), and a C++ toolchain requirement from a tokenizer dependency.
>
> Numbers: on a demo crate, `cargo test` goes from 9,026 to 660 bytes with the failing assertion kept and the exit code preserved. Token counts are bytes / 4 estimates.
>
> Apache-2.0: https://github.com/phuetz/lm-resizer. Feedback on the crate layout and the proxy design welcome.

### 6.3 r/LocalLLaMA (mar. 24/11, 15 h 30 Paris, optionnel)

Rappels : auto-promo ≤ 10 % de l'activité ; karma minimum ; divulguer l'usage de LLM. **Ne poster qu'avec une mesure locale réelle** `[à vérifier : faire un test Ollama avec un contexte 8–16k]`.

**Title:** `Small local context windows suffer most from noisy tool output: a local proxy that compresses it before it reaches Ollama (Rust, Apache-2.0)`

> I'm the author. If you run coding agents on local models, an 8k-32k window fills up fast with `cargo test`, `npm install` or `git diff` output. lm-resizer is a local Rust binary that filters that output before it reaches the model, either as a command wrapper (`lm-resizer exec -- cargo test`) or as a local proxy in front of any OpenAI-compatible endpoint:
>
> ```
> lm-resizer serve --upstream http://127.0.0.1:11434/v1 --dashboard
> ```
>
> The proxy was validated end-to-end on 2026-06-23 in front of Ollama (and Mistral, DeepSeek, OpenRouter, xAI). Query-aware mode (`-q`) biases what is kept toward the current question when something must be dropped; dropped blocks stay recoverable from a local SQLite store. No telemetry, no account.
>
> `[insérer : mesure réelle avec modèle local, taille de contexte, avant/après]`
>
> https://github.com/phuetz/lm-resizer. What's the worst tool output you've had to fit into a local model's context?

### 6.4 r/programming (mer. 25/11, 15 h 00 Paris, optionnel)

r/programming n'accepte pas bien les annonces de produit : poster un **article** (dev.to / blog) plutôt que le dépôt `[à vérifier : règles du sub]`.

**Title:** `What coding agents actually read: measuring 32k real commands (command-output filtering saves ~60% of bytes, not 90%)`

> Link post to the write-up. Summary for the comments: I logged the output of 32,483 commands run by coding agents on my own projects and measured what a filtering layer (lm-resizer, Rust, Apache-2.0, my project) removes. Overall about 59.5% of bytes, with huge variance: JS lint/quality tools ~99%, generic output ~27%, `git status` ~3%, and some search results that grew after annotation, which led to a "never grow" gate. Token savings are bytes / 4 estimates. Happy to discuss methodology. `[chiffres à remplacer par la mesure refaite après la PR #18]`

### 6.5 Show HN (mar. 17/11, 15 h 00 Paris = 9 h 00 ET = 6 h 00 PT)

**Title (76 caractères):** `Show HN: lm-resizer – filter noisy tool output before it reaches Claude Code`

**URL:** `https://github.com/phuetz/lm-resizer`

**First comment (author):**

> Hi HN, I'm Patrice. lm-resizer is a local Rust binary that sits between the commands a coding agent runs and the model. Claude Code and Codex spend a lot of context on `cargo test`, `npm install`, `git diff` or `rg` output; most of it is "ok" lines and repeated noise that costs tokens and latency and can hide the one error that matters.
>
> What it does: per-family filters (git, cargo, rg, vitest/jest, Terraform, kubectl, Go, .NET, JVM, Playwright...) followed by compression. Exit codes are preserved. Raw output goes to a local file, and compressed blocks to a local SQLite store, recoverable with `lm-resizer retrieve <hash>`.
>
> Ways to use it: `lm-resizer exec -- cargo test`; native Claude Code / Codex hooks (`init-native-hooks`) that rewrite supported commands and run unknown ones raw; an MCP server; opt-in PATH shims; or a local proxy (`serve`) in front of OpenAI/Anthropic-compatible APIs, with a small dashboard.
>
> Numbers, with scope: on a demo crate (300 passing + 1 failing test), 323 lines / 9,026 bytes become 19 lines / 660 bytes, failing assertion intact. Over a session of 398 commands routed through it (cumulative): 1.23 MB -> 372 KB. Token figures are bytes / 4 estimates, like RTK. `[insérer le chiffre global refait après la PR #18 : ~59.5% on 32k commands, with variance]`
>
> Prior art: RTK and Headroom are the reference projects and far more mature. lm-resizer borrows ideas from both (RTK-style command filters, Headroom-style reversible compression) and puts wrapper, hooks, MCP, proxy and recovery in one binary. `[aligner avec la section Acknowledgements du README]`
>
> Limits: young, one maintainer; build from source for now (recent stable Rust + a C++ compiler); ML content detection is optional and off by default. Apache-2.0, local, no telemetry.
>
> I'd love to hear which commands still flood your agent's context, and whether the hook should be more or less aggressive.

## 7. Checklist de diffusion

Heures : à partir du 1/11, Paris = CET (UTC+1) → **ET = Paris − 6 h, PT = Paris − 9 h**. Jours à éviter : 11/11 (férié FR), 26–27/11 (Thanksgiving / Black Friday US). **Pas de Product Hunt en novembre** pour LM Resizer.

### 7.1 Calendrier croisé des lancements

| Date | Produit | Événement |
|---|---|---|
| jeu. 1/10 | Code Buddy | Release |
| mar. 6/10, 15 h 00 | Code Buddy | Show HN |
| mar. 20/10, 15 h 00 | Code Explorer | Show HN |
| lun. 26/10 → ven. 30/10 | LM Resizer | Préparation (décisions, README, binaires) — rien de public |
| sem. du 2/11 (Product Hunt Code Buddy ~mar. 3/11) | Code Buddy | **Gel LM Resizer : aucune publication publique** ; pas de posts Code Explorer 2–5/11 non plus |
| mar. 10/11 (J-7) | LM Resizer | Tag + release + npm |
| **mar. 17/11 (J0)** | **LM Resizer** | **Show HN 15 h 00** |
| mar. 1/12 ou 8/12 | Code Explorer | Product Hunt |

### 7.2 Préparation

- [ ] **Avant ven. 30/10** : trancher la PR #14 (licence) ; fusionner la PR #18 ; décider pour les PR #15–#17 (ne rien fusionner de gros après le 10/11).
- [ ] **Avant ven. 30/10** : aligner README / `SKILL.md` / `plugin.json` sur le binaire (retirer ou livrer `tool-output`, `exec --token-budget`, `POST /tool-output`, `lm_resizer_tool_output`).
- [ ] Refaire une capture datée de `lm-resizer stats --markdown` ; corriger le hero README (398 = cumul ; 222 247 vs 214 938) ; corriger `docs/marketing/LINKEDIN-2026-09-lm-resizer.md` (terme proscrit l. 31 et hashtag l. 39) et `VIDEO-LISA-2026-09-lm-resizer.md`.
- [ ] Section « Acknowledgements » (RTK, Headroom) + fichier NOTICE si nécessaire `[à vérifier]`.
- [ ] `--version` ; `rust-version = "1.86"` ; champ `license` dans `packages/wasm/package.json` ; versions `plugin.json` / `marketplace.json`.
- [ ] Binaires v0.3.0 (Linux x86_64/aarch64, macOS arm64, Windows) + `SHA256SUMS`.
- [ ] Tester `claude plugin marketplace add phuetz/lm-resizer` + `claude plugin install lm-resizer@phuetz-tools` sur une machine propre.
- [ ] Rejouer `node packages/wasm/smoke.mjs` (3 161 → 1 222 octets) après build WASM.
- [ ] Tournage (VHS + OBS), montage 16:9 / 9:16 / GIF, sous-titres FR/EN, image sociale 1280×640.
- [ ] Karma r/ClaudeAI : commenter utilement dès fin octobre (hors semaine du 2/11 pour les posts, commentaires OK).

### 7.3 Plan jour par jour

| Jour | Date | Heure Paris | ET / PT | Action |
|---|---|---|---|---|
| — | lun. 26/10 → ven. 30/10 | — | — | Décisions licence / PR #18, alignement README, binaires |
| Gel | lun. 2/11 → ven. 6/11 | — | — | **Aucune publication publique LM Resizer** (Product Hunt Code Buddy) ; travail interne seulement |
| J-7 | mar. 10/11 | 10 h 00 | 4 h 00 / 1 h 00 | Tag **v0.3.0**, release GitHub avec binaires, publication npm via `publish-wasm.yml` (OIDC) ; vérifier la provenance ; gel du périmètre |
| J-5 | jeu. 12/11 | — | — | Tournage final avec le binaire v0.3.0 |
| J-2 | dim. 15/11 | — | — | Test d'installation propre (3 OS) depuis la release ; plugin Claude |
| J-1 | lun. 16/11 | — | — | Relecture ; chiffres et étoiles actualisés ; décision licence reflétée partout ; recompter les tweets (`tweets.py`) |
| **J0** | **mar. 17/11** | **15 h 00** | **9 h 00 / 6 h 00** | **Show HN** + premier commentaire immédiat ; disponible jusqu'à 19 h |
| J0 | mar. 17/11 | 16 h 30 | 10 h 30 / 7 h 30 | **Thread X** (vidéo dans le tweet 1, lien seulement dans le 6) |
| J+1 | mer. 18/11 | 8 h 30 | 2 h 30 / — | **LinkedIn** (lien en commentaire) |
| J+1 | mer. 18/11 | 15 h 30 | 9 h 30 / 6 h 30 | **r/ClaudeAI** (flair, disclosure) |
| J+2 | jeu. 19/11 | 10 h 00 | 4 h 00 / 1 h 00 | Journal du Hacker (optionnel, lien vers un article FR) |
| J+3 | ven. 20/11 | 15 h 00 | 9 h 00 / 6 h 00 | **r/rust** (retour d'expérience technique) ; alternative : jeu. 19/11 même heure |
| J+6 | lun. 23/11 | 9 h 00 | 3 h 00 / 0 h 00 | LinuxFr.org, journal (optionnel) |
| J+7 | mar. 24/11 | 15 h 30 | 9 h 30 / 6 h 30 | **r/LocalLLaMA** (seulement avec mesure locale réelle) |
| J+8 | mer. 25/11 | 15 h 00 | 9 h 00 / 6 h 00 | **r/programming** (article de mesure) |
| — | décembre | — | — | Pas de Product Hunt LM Resizer en novembre ; à rediscuter après le Product Hunt Code Explorer (1/12 ou 8/12) |

### 7.4 Métriques et réponses

- Étoiles/jour, trafic GitHub (`traffic/views`, `traffic/popular/referrers`, conservés 14 jours : exporter chaque semaine), téléchargements des assets de release, npm hebdo.
- Réponses types : « Pourquoi pas RTK ? » → RTK est la référence terminal ; lm-resizer ajoute MCP, proxy, compression orientée requête et store de récupération, et coexiste avec RTK. « Port de Headroom ? » → réponse factuelle + lien Acknowledgements. « Vos tokens ? » → estimation octets / 4, chiffres avec périmètre. « Et si le filtre cache l'info ? » → `--raw-on-failure`, `tee read`, `retrieve <hash>`, issue avec la sortie.
- Critique juste → « bon point, issue #N », correctif visible dans la semaine.
