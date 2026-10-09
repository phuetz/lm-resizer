# LM Resizer

**Raccourcissez les sorties de commande bruyantes avant qu'elles n'atteignent votre agent de code — sans jamais perdre un seul échec.** LM Resizer est un CLI Rust local et rapide, destiné aux développeurs qui pilotent tests, compilations, Git, conteneurs et autres outils depuis un agent de code comme Claude Code, Codex, Cursor, Gemini CLI, un client MCP ou un pipeline maison. Il lance une commande, garde un résultat compact pour l'agent et conserve l'original exact pour un rappel immédiat.

Sur un banc de 61 captures de commandes, il économise en moyenne **28,39 %** de jetons (médiane **4,87 %**), rappel du brut compris, conserve le code de sortie du producteur dans **61/61** cas et démarre en environ **5 ms** (`lm-resizer --version` ; un `exec` lance aussi la commande enveloppée et prend environ **20 ms**). Zéro télémétrie, 100 % local, déterministe.

![Exemple de traitement d'une sortie de commande par LM Resizer](docs/lm-resizer-hero.png)

[English](README.md) · [Banc de comparaison](bench/native/README.md)

## Installation

Binaire précompilé pour Linux et macOS :

~~~sh
curl -fsSL https://raw.githubusercontent.com/phuetz/lm-resizer/v0.2.6/install.sh -o install.sh && sh install.sh
export PATH="$HOME/.local/bin:$PATH"
lm-resizer --version
~~~

Windows PowerShell :

~~~powershell
irm https://raw.githubusercontent.com/phuetz/lm-resizer/v0.2.6/install.ps1 | iex
~~~

L'installeur vérifie la somme SHA-256 de l'archive et la version du binaire avant de poser `lm-resizer` dans `~/.local/bin` par défaut. Plateformes préparées : Linux x86_64, macOS x86_64/arm64 et Windows x86_64.

Sur Linux et macOS, l'installeur affiche la ligne à ajouter quand `~/.local/bin` n'est pas dans le `PATH` ; pour la rendre permanente sous Bash, lancez `echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc` (`~/.zshrc` pour zsh) puis ouvrez un nouveau terminal. L'installeur Windows met à jour le `PATH` utilisateur. `LM_RESIZER_INSTALL_DIR` permet de choisir un autre répertoire. Pour désinstaller un binaire précompilé, retirez-le de ce répertoire. [Windows : repli TLS, PowerShell 5.1 et récupération exacte](docs/WINDOWS.md).

## Voyez le résultat

`tool-output` filtre une sortie déjà capturée par un hôte. La vue réduite conserve le test en échec et pointe vers l'original exact :

~~~console
$ lm-resizer tool-output --command 'cargo test' --input bench/corpus/cargo_fail.txt
FAILURES (1):
1. ---- parse::reject_empty stdout ----
thread 'parse::reject_empty' panicked at src/parser.rs:42:9:
assertion `left == right` failed: expected=422 observed=200

test result: FAILED. 70 passed; 1 failed; finished in 0.09s
[tee:78bf04f25902] lm-resizer tee read 78bf04f25902
~~~

La même commande sur le fichier de 78 lignes compte **792** jetons d'origine et **80** jetons compressés. Quatre captures fournies, mesurées sur cette machine avec `o200k_base` :

| Capture (`bench/corpus/`) | `--command` | Origine | Compressé |
|---|---|---:|---:|
| `cargo_ok.txt` | `cargo test` | 837 | 24 |
| `cargo_fail.txt` | `cargo test` | 792 | 80 |
| `pytest_ok.txt` | `pytest` | 937 | 17 |
| `git_log.txt` | `git log` | 2320 | 2320 |

Reproduisez n'importe quelle ligne avec `lm-resizer tool-output --command '<command>' --input <file> --json` : le JSON expose `original_tokens`, `compressed_tokens` et `tokens_saved` avec `token_count_method: "exact"`. `git_log.txt` revient inchangé : la vue de `git log` montre chaque commit et, sur cet historique, n'économise aucun jeton ; le brut est donc conservé.

## Pourquoi LM Resizer ? (face à un filtre de référence et à Headroom)

Deux autres outils servent souvent à réduire les sorties de commande : un filtre de sorties de commandes (la référence épinglée, nommée dans [le banc](bench/native/README.md)) et Headroom. Leurs chiffres publiés sont reproduits dans [`bench/native/windows-release/delivery.md`](bench/native/windows-release/delivery.md) sur les mêmes 61 captures, tokenisées avec `o200k_base`, rappel du brut compris :

| 61 captures de commandes, `o200k_base` | LM Resizer 0.2.6 | Filtre de référence épinglé | Headroom 0.39.1, API générale |
|---|---:|---:|---:|
| Médiane de jetons économisés, rappel du brut compris | 4,87 % | 15,81 % | 0,00 % |
| Moyenne de jetons économisés, rappel du brut compris | 28,39 % | 32,61 % | 1,91 % |
| Code de sortie du producteur conservé | 61/61 | 52/61 | non mesuré |
| Brut exact récupérable | 61/61 | partiel ou absent | non |

La médiane (4,87 %) et la moyenne (28,39 %) rejouées de LM Resizer sont toutes deux inférieures à celles du filtre de référence (15,81 % et 32,61 %) sur ce corpus. Depuis la 0.2.6, `git log` n'est raccourci que par `exec`, sur un format par défaut prouvé ; `pipe` et `tool-output` le rendent brut, alors que le filtre de référence n'en montre toujours que le premier commit (cinq captures qui comptaient 91 à 97 % d'économie en comptent maintenant 0 %). Les grands patchs gardent leur début diagnostique et leur brut intégral reste dans tee ; les assertions et diagnostics du compilateur restent entiers. Le filtre de référence retourne le code de sortie 0 dans **9** des 61 captures où le producteur échoue ; LM Resizer conserve le code du producteur. Sur une capture, LM Resizer rend 251 jetons contre 40 chez le filtre de référence parce qu'il indique les **457** erreurs de collecte et liste les dix premiers fichiers en échec, ce qu'il omet. Headroom compresse sémantiquement le contexte d'API LLM et n'a pas de filtres dédiés aux outils CLI : sa médiane d'économie sur ce corpus de commandes est de 0,00 %. L'égalité stricte des vues est de 36/61 ; les vingt-cinq écarts et leurs raisons sont listés dans le fichier du banc, et le contrôle strict du banc sort en code 1 par construction. Les chiffres se rejouent en une étape avec `bench/real/rejouer.sh` (Python isolé avec tiktoken, exécutable de comparaison construit depuis l'archive épinglée, rejeu dans un dossier neuf, synthèse des médianes, récupérations et codes de sortie ; réseau nécessaire la première fois, `--sans-headroom` omet la colonne Headroom) ; le script Python du banc seul exige ses arguments, voir [le banc](bench/native/README.md).

## Essayez sur votre projet

`exec` lance une commande, donne à l'agent la vue raccourcie et conserve l'original. Les exemples ci-dessous supposent que les outils enveloppés sont présents : `git` et un répertoire courant dans un dépôt Git (hors dépôt, Git échoue lui-même avec le code 128 et il n'y a rien à raccourcir), et un projet Rust avec Cargo pour `cargo test`. Si une commande enveloppée n'est pas installée, `exec` affiche `cannot execute <name>: command not found` et sort avec 127.

Vérifiez que l'installation fonctionne partout, sans dépôt :

~~~bash
lm-resizer --version
lm-resizer exec -- echo hello
lm-resizer tee list
~~~

Sous Windows `echo` est une commande interne de `cmd` et de PowerShell, pas un programme, et `exec` ne peut pas la lancer : utilisez `lm-resizer exec -- cmd /c echo hello`.

Ensuite, depuis un dépôt Git et depuis un projet Rust :

~~~bash
lm-resizer git log -20
lm-resizer exec --raw-on-failure -- cargo test
lm-resizer gain --history --project
~~~

`gain` peut commencer négatif : sur une sortie minuscule, la vue et la ligne de rappel coûtent plus de jetons que l'original. Les vraies sorties rendent le total positif.

Pour toute commande, `lm-resizer err|test|summary -- <command>` garde les diagnostics ou bilans de tests avec une ligne de contexte et le code d’échec en tête. `exec` et `tool-output` résument aussi les commandes sans filtre dédié, sauf les lignes de shell comme `sh -c`, rendues brutes. Chaque sortie reste récupérable avec `tee read`. `gain` affiche les commandes et jetons mesurés ; `gain --json` donne les compteurs complets. Voir la [référence CLI](docs/CLI-REFERENCE.md).

Pour les scripts, utiliser `lm-resizer gain --json`.

`exec` conserve le statut du producteur (128 + signal sous Unix). `--stream` et `--raw-on-failure` gardent stdout et stderr séparés et affichent le marqueur `[stderr]` ; la vue raccourcie par défaut ne le fait pas, utilisez l'une des deux quand l'origine d'une ligne compte.

## La garantie diagnostique

LM Resizer raccourcit, il ne cache pas. Les vues de commandes conservent littéralement nombres, chemins, identifiants, auteurs de commit et diagnostics d'échec. Les réussites reconnues de Cargo/pytest peuvent être résumées par des compteurs de suite ; les échecs restent visibles. Si une étape de compression devait omettre une ligne d'échec, le corps filtré est conservé et, quand le gain le permet, une ligne `[tee:<id>]` pointe vers l'original intact. Une vue qui contient un diagnostic peut donc n'afficher presque aucun gain — c'est voulu. Les noms d'étapes internes (`diagnostic-guard`, `kept_filtered`, `diagnostic_reinjection`) et le chemin `raw_on_failure` sont listés dans [la référence de la ligne de commande](docs/CLI-REFERENCE.md).

`lm-resizer expand -i view.txt` reconstruit les vues réversibles, dont les historiques et diffs `Patch v1` et `Patch v2` : les en-têtes communs et répétitions sont factorisés sans retirer de ligne de source ni de contexte.

## Récupérer la sortie exacte

`exec` archive stdout et stderr entrelacés jusqu'à EOF, sans plafond de 10 Mio. Une vue suffisamment réduite peut afficher `[tee:<id>]` ; le rappel se lit avec `lm-resizer tee read <id>` ; sinon `tee list` et le champ JSON `tee_hint` donnent accès au brut sans alourdir la vue.

`tee list` est trié par nom de fichier, qui est une empreinte du contenu, pas par date : sa première entrée n'est pas « la dernière sortie ». Prenez l'identifiant dans la vue (`[tee:<id>]`), dans le champ `tee_hint` d'un rapport `--json` ou dans le marqueur `[raw: …]`, puis lisez-le :

~~~bash
lm-resizer tee list
lm-resizer tee read e3b0c44298fc
~~~

`lm-resizer --version` affiche `lm-resizer 0.2.6`. Le rappel JSON porte un identifiant tel que `[raw: e3b0c44298fc]` ; les archives ont l'extension `.log`.

## Intégrations agents

Le CLI propose aussi `compress` pour des fichiers ou l'entrée standard et `tool-output` pour une sortie déjà capturée, plus des intégrations MCP, HTTP et hooks d'agents activées sur demande. `install --client all --scope project` écrit aussi la configuration utilisateur Codex et remplace une table `mcp_servers.lm_resizer` existante sans sauvegarde ; conservez-en une copie avant installation. Voir le [guide des intégrations agents](docs/CLAUDE_CODEX.md), le [guide de release](docs/RELEASE.md) et les [hooks d'agents supplémentaires](docs/AGENT_HOOKS.md) pour ces usages. Chaque option, commande et variable affichée par `--help` est listée dans [la référence de la ligne de commande](docs/CLI-REFERENCE.md).

Exemples à copier depuis ce checkout (Bash) :

~~~bash
printf 'hello world\n' | lm-resizer compress
git log -20 '--format=Date: %ad%n%h %s' --date=short | lm-resizer tool-output --command 'git log -20'
~~~

`compress` lit le texte de stdin ; `tool-output` filtre une sortie déjà capturée. Son argument `--command` décrit la commande et ne l'exécute pas. Une petite sortie peut rester intacte.

Pour configurer les quatre clients MCP (Claude Code, Codex, Cursor et VS Code) depuis la racine du projet, après avoir sauvegardé toute configuration Codex existante :

~~~bash
lm-resizer install --client all --scope project
~~~

Cette commande écrit les fichiers du projet et la configuration Codex de votre compte, y compris avec `--scope project`. Gemini CLI n'est pas couvert par `install` : utilisez `lm-resizer init --client gemini --project-dir .` ([guide des hooks d'agents](docs/AGENT_HOOKS.md)).

### Crochets et permissions

Les crochets d'agent (`lm-resizer init-native-hooks`, `lm-resizer install-hooks`) réécrivent une commande Bash prise en charge en `lm-resizer exec -- <command>`. Pour Claude Code le crochet ne fait que réécrire : il n'accorde aucune permission, donc Claude Code demande l'accord pour la commande réécrite comme pour toute autre (vérifié sur Claude Code 2.1.294). Seul Codex exige que le crochet réponde `permissionDecision: allow`. La commande réécrite ne correspond plus à une règle de permission écrite pour l'original : **une règle `deny` comme `Bash(cargo test)` n'arrête pas `lm-resizer exec -- cargo test`**. Ajoutez aussi une règle deny pour la forme enveloppée, par exemple `Bash(*exec -- cargo test*)`, ou n'installez pas le crochet. Voir [SECURITY.md](SECURITY.md).

Le crochet ne réécrit une commande que si c'est une commande simple. Avec `cd <dir> && …`, un tube comme `| tail`, une redirection ou `&&`, il laisse la ligne brute : rien n'est raccourci et rien ne casse. Les agents écrivent souvent `cd <dir> && cargo test 2>&1 | tail` ; pour profiter du crochet, demandez-leur de lancer `cargo test` seul depuis le dossier du projet.

## Statistiques de jetons reproductibles

`lm-resizer stats --markdown` affiche les comptes exacts du texte avec le tokenizer existant **tiktoken-rs / o200k_base** (famille GPT-4o). Les JSON `exec`, `tool-output` et `compress` exposent `original_tokens`, `compressed_tokens`, le gain signé `tokens_saved`, `tokenizer` et `token_count_method: "exact"`. Le compte inclut les marqueurs de récupération finaux ; un gain négatif signifie davantage de jetons en sortie. Cet encodage de référence ne mesure ni le tokenizer de Claude/Llama ni une facture fournisseur.

Les nouvelles entrées d'historique conservent les deux comptes. Les statistiques gardent les champs JSON existants d'octets et ajoutent les totaux mesurés et `measured_commands` / `unmeasured_commands`. Les anciennes entrées ne contiennent pas le texte à recompter : leur `estimated_tokens_saved` reste une estimation historique explicitement libellée, séparée des mesures. `discover`, `discover-sessions`, `eval` et `learn` comptent le texte original et filtré disponible : il s'agit de gains potentiels du filtre. Pour la compatibilité JSON, leur champ `estimated_tokens_saved` est un alias du gain potentiel réellement compté `tokens_saved`.

[Méthode, compatibilité et reproduction](docs/TOKEN-STATISTICS.md). Le banc de parité ci-dessus utilise `o200k_base`.

## Filtres natifs et mesures actuelles

Le produit utilise ses propres filtres Rust et TOML. Les commandes d'inspection explicites incluent `err`, `test`, `summary`, `json`, `deps`, `env`, `format`, `outline` et `dedup`. Les lectures de fichiers restent littérales. Les plis réversibles de chemins et correspondances, tables JSON et répétitions exactes complètent les filtres de commandes ; le contour syntaxique et la déduplication de blocs sont explicites.

**Moyenne tee compris : 28,39 %.** La médiane est de 4,87 % ; les 61 récupérations du brut et les codes de sortie du producteur passent tous, et le démarrage mesure environ 5 ms pour `lm-resizer --version` et environ 20 ms pour `lm-resizer exec -- echo hello` (médiane de 60 lancements, Linux, build release, machine chargée). Ces médianes de corpus ne sont pas une affirmation sur des dépôts vivants arbitraires. [Mesures actuelles, écarts exacts et limites](bench/native/windows-release/delivery.md).

`env` masque les noms contenant `PASSPHRASE` (y compris `PASSPHRASE_FILE`) et un composant de nom `PASS`. Cela masque volontairement aussi des noms inoffensifs comme `PASS_COUNT` ; le filtrage est prudent, fondé sur les noms et les formes d'URL à identifiants.

## Compiler depuis les sources

Le CLI requiert Rust **1.91 ou plus récent** ; `rust-toolchain.toml` épingle **1.95.0**, avec rustfmt et clippy. Utilisez rustup officiel si le compilateur de la distribution est trop ancien.

Prérequis système à installer avant Rust :

- **Debian 13 / Ubuntu** : `curl`, certificats HTTPS, Git, compilateur C (`cc`/`gcc`), compilateur C++ (`c++`/`g++`), en-têtes de la libc et éditeur de liens (`binutils`, installé avec GCC). Dans un terminal avec sudo (ou comme root sans `sudo`) :

~~~bash
sudo apt-get update
sudo apt-get install -y --no-install-recommends ca-certificates curl git gcc g++ libc6-dev
~~~

- **macOS** : installez les outils de ligne de commande Xcode (`xcode-select --install`) ; ils fournissent Git, Clang/Clang++, les en-têtes et le SDK. Terminez la boîte de dialogue d'installation avant de poursuivre.
- **Windows x86_64** : installez [Git for Windows](https://git-scm.com/downloads/win) et [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022) avec **Développement Desktop en C++**, les outils **MSVC C++ x64/x86** et le **SDK Windows**. Si ces composants sont déjà présents, conservez-les.

La compilation est vérifiée sous Linux x86_64 avec Rust 1.95.0. Les autres plateformes restent à vérifier.

Téléchargez les sources du tag public (Bash ou PowerShell). `lm-resizer` garde son état dans `~/lm-resizer` : clonez depuis un autre dossier (par exemple `mkdir -p ~/src && cd ~/src` sous Bash, ou `New-Item -ItemType Directory -Force "$HOME\src" | Set-Location` sous PowerShell) plutôt que depuis votre dossier personnel :

~~~sh
git clone --branch v0.2.6 https://github.com/phuetz/lm-resizer.git
cd lm-resizer
~~~

Les blocs Rust ci-dessous s'exécutent depuis la racine de ce checkout. Si vous êtes déjà dans un checkout, ne le clonez pas une deuxième fois.

Linux/macOS (Bash) :

~~~bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path --profile minimal --default-toolchain 1.95.0
. "$HOME/.cargo/env"
cargo --version
cargo install --quiet --path . --locked --root "$HOME/.local"
export PATH="$HOME/.local/bin:$PATH"
lm-resizer --version
~~~

Windows x86_64 (PowerShell) :

~~~powershell
$rustupInstaller = Join-Path $env:TEMP 'rustup-init.exe'
Invoke-WebRequest -Uri 'https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe' -OutFile $rustupInstaller
& $rustupInstaller -y --no-modify-path --profile minimal --default-toolchain 1.95.0
$env:Path = "$(Join-Path $env:USERPROFILE '.cargo\bin');$env:Path"
cargo --version
$installRoot = Join-Path $env:USERPROFILE '.local'
cargo install --quiet --path . --locked --root "$installRoot"
$env:Path = "$installRoot\bin;$env:Path"
lm-resizer --version
~~~

Si Rust manque ou est trop ancien, les étapes rustup au début du bloc de votre plateforme installent la toolchain indiquée par le dépôt. Si rustup est déjà installé et son dossier bin dans le PATH, vous pouvez les omettre. `--no-modify-path` laisse vos fichiers de profil et le PATH permanent intacts ; les commandes suivantes règlent seulement le PATH du terminal courant. Avec le Rust de la distribution déjà installé, rustup peut afficher `cannot install while Rust is installed`, puis `continuing (because the -y flag is set and the error is ignorable)`. rustup déclare l'erreur ignorable et continue grâce à `-y` : vérifiez le code de sortie et `cargo --version` après activation de `.cargo/env` ; il doit afficher 1.95.0. Le Rust système reste installé. [Instructions officielles Rust](https://www.rust-lang.org/tools/install/).

Cargo télécharge ses dépendances dans `~/.cargo` (`%USERPROFILE%\.cargo` sous Windows) même si une compilation échoue ; rustup conserve les compilateurs dans `.rustup`. Ces caches sont distincts du préfixe `.local`. Si Cargo n'est pas en 1.95.0 dans ce checkout, vérifiez le PATH (`command -v cargo` sous Bash, `Get-Command cargo` sous PowerShell), `rustup show active-toolchain` et toute surcharge `RUSTUP_TOOLCHAIN`. Arrêtez-vous si `cargo install` échoue : les exemples et la désinstallation ci-dessous nécessitent un binaire effectivement installé. Si `~/.local/bin/lm-resizer` existe déjà (par exemple après une installation précompilée), Cargo refuse avec `binary lm-resizer already exists in destination` ; retirez ce fichier ou relancez avec `--force`.

Pour retirer un binaire installé avec Cargo :

~~~bash
cargo uninstall --root "$HOME/.local" lm-resizer
~~~

Windows (PowerShell) :

~~~powershell
cargo uninstall --root "$installRoot" lm-resizer
~~~

## Désinstaller

Défaites ce que vous avez installé, dans cet ordre.

1. Les crochets, projet par projet :

~~~bash
lm-resizer uninstall-hooks --client all --project-dir .
~~~

   Cette commande retire les blocs de consignes, les scripts générés et les fichiers de crochets natifs s'ils sont encore exactement ceux qui ont été générés ; un fichier modifié à la main est laissé en place.

2. Les serveurs MCP. Il n'y a pas de commande : supprimez à la main l'entrée `lm-resizer` de `.mcp.json` et `.cursor/mcp.json` (clé `mcpServers`), de `.vscode/mcp.json` (clé `servers`) et la table `[mcp_servers.lm_resizer]` de `~/.codex/config.toml`.

3. Les données enregistrées. Les archives de sortie brute, l'historique des commandes et la base CCR sont dans le dossier d'état : `~/lm-resizer` par défaut, ou `XDG_STATE_HOME`, `LOCALAPPDATA` ou `LM_RESIZER_STATE_DIR` s'ils sont définis. Supprimez les archives, puis le dossier :

~~~bash
lm-resizer tee purge --all
~~~

4. Le binaire : `cargo uninstall` comme indiqué dans « Compiler depuis les sources », ou supprimez `~/.local/bin/lm-resizer`.

## Quand ne pas l'utiliser

- Ne prenez pas une vue raccourcie pour une piste d'audit complète : inspectez l'original conservé pour la sécurité, la conformité ou les échecs subtils.
- N'attendez pas un gain sur toute entrée : les sorties courtes ou peu répétitives peuvent rester intactes.
- La réduction des jetons de sortie ne garantit pas une baisse directement proportionnelle de vos coûts d'API ni le succès de l'agent : la facturation réelle dépend aussi du cache de contexte multi-tours et de l'architecture des invites. Ni les tâches d'agent ni la facturation n'ont été testées ici.

LM Resizer est sous licence Apache-2.0. [Contribution](CONTRIBUTING.md) · [Sécurité](SECURITY.md)

[Crédits et licences des inspirations](THIRD-PARTY-NOTICES).
