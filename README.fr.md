# LM Resizer

**Raccourcir les sorties de commande bruyantes avant qu'elles n'arrivent à un agent de code.** LM Resizer est un outil Rust local qui peut lancer une commande, donner un résultat compact à l'agent et conserver la sortie originale pour inspection. Il traite les journaux de tests, les sorties Git, les diagnostics de compilation, le JSON et d'autres textes ; le résultat dépend de l'entrée et de la commande.

![Exemple de traitement d'une sortie de commande par LM Resizer](docs/lm-resizer-hero.png)

[English](README.md) · [Banc de comparaison](bench/native/README.md)

[Windows : repli TLS, PowerShell 5.1 et récupération exacte](docs/WINDOWS.md).

## Installer et essayer

### Binaire précompilé, après la publication de v0.2.4

**Ne lancez pas les commandes ci-dessous avant la publication de la release v0.2.4 et de ses archives : elles renvoient actuellement HTTP 404. Pour installer maintenant, suivez « Compiler depuis les sources » plus bas.**

Après cette publication seulement, utilisez la commande de votre plateforme. L'installeur vérifie la somme SHA-256 de l'archive et la version du binaire avant de poser `lm-resizer` dans `~/.local/bin` par défaut. Plateformes préparées : Linux x86_64, macOS x86_64/arm64 et Windows x86_64. La release v0.2.2 existante n'a pas d'archives précompilées.

Linux et macOS. Prérequis : un `sh` POSIX (Bash n'est pas nécessaire), `tar`, `gzip`, et `curl` ou `wget`. Les images minimales Debian/Ubuntu n'ont ni l'un ni l'autre (Alpine fournit un `wget` limité) : installez d'abord curl (`sudo apt-get update && sudo apt-get install -y curl ca-certificates` sous Debian/Ubuntu, `sudo apk add curl` sous Alpine ; sans `sudo` si vous êtes root, comme dans un conteneur). Avec wget à la place de curl, téléchargez le fichier par `wget -qO install.sh https://raw.githubusercontent.com/phuetz/lm-resizer/v0.2.4/install.sh`, puis lancez `sh install.sh`. Le binaire Linux x86_64 est statique (ni glibc ni libstdc++) : il démarre sur Debian 12, Ubuntu 20.04+ et Alpine.

~~~sh
curl -fsSL https://raw.githubusercontent.com/phuetz/lm-resizer/v0.2.4/install.sh -o install.sh && sh install.sh
~~~

Windows PowerShell :

~~~powershell
irm https://raw.githubusercontent.com/phuetz/lm-resizer/v0.2.4/install.ps1 | iex
~~~

Sur Linux/macOS, l'installeur affiche la ligne à ajouter quand `~/.local/bin` n'est pas dans le `PATH` ; pour la rendre permanente sous Bash : `echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc` (`~/.zshrc` pour zsh), puis ouvrez un nouveau terminal. Pour le terminal courant seulement :

~~~sh
export PATH="$HOME/.local/bin:$PATH"
lm-resizer --version
~~~

L'installeur Windows met à jour le `PATH` utilisateur. `LM_RESIZER_INSTALL_DIR` permet de choisir un autre répertoire. Pour désinstaller un binaire précompilé, retirez-le de ce répertoire.

### Compiler depuis les sources

Le CLI requiert Rust **1.91 ou plus récent** ; `rust-toolchain.toml` épingle **1.95.0**, avec rustfmt et clippy. Utilisez rustup officiel si le compilateur de la distribution est trop ancien.

Prérequis système à installer avant Rust :

- **Debian 13 / Ubuntu** : `curl`, certificats HTTPS, Git, compilateur C (`cc`/`gcc`), compilateur C++ (`c++`/`g++`), en-têtes de la libc et éditeur de liens (`binutils`, installé avec GCC). Dans un terminal avec sudo (ou comme root sans `sudo`) :

~~~bash
sudo apt-get update
sudo apt-get install -y --no-install-recommends ca-certificates curl git gcc g++ libc6-dev
~~~

- **macOS** : installez les outils de ligne de commande Xcode (`xcode-select --install`) ; ils fournissent Git, Clang/Clang++, les en-têtes et le SDK. Terminez la boîte de dialogue d’installation avant de poursuivre.
- **Windows x86_64** : installez [Git for Windows](https://git-scm.com/downloads/win) et [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022) avec **Développement Desktop en C++**, les outils **MSVC C++ x64/x86** et le **SDK Windows**. Si ces composants sont déjà présents, conservez-les.

La compilation est vérifiée sous Linux x86_64 avec Rust 1.95.0. Les autres plateformes restent à vérifier.

Si Rust manque ou est trop ancien, exécutez les étapes d’installation de rustup au début du bloc de votre plateforme. Si rustup est déjà installé et son dossier bin dans le PATH, vous pouvez les omettre : il installera la toolchain indiquée par le dépôt. L’installeur ci-dessous sélectionne 1.95.0 par défaut dans votre compte ; `--no-modify-path` laisse vos fichiers de profil et le PATH permanent intacts. Les commandes suivantes règlent seulement le PATH du terminal courant. [Instructions officielles Rust](https://www.rust-lang.org/tools/install/).

Avant publication, partez d’un checkout de la candidate `feat/filtres-natifs-2026-10-03` fourni par le mainteneur. Cette branche locale n’est pas nécessairement disponible sur GitHub. **Après publication seulement**, téléchargez les sources du tag public (Bash ou PowerShell) :

~~~sh
git clone --branch v0.2.4 https://github.com/phuetz/lm-resizer.git
cd lm-resizer
~~~

Les blocs Rust ci-dessous s’exécutent depuis la racine de ce checkout. Si vous êtes déjà dans un checkout, ne le clonez pas une deuxième fois.

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

Avec le Rust de la distribution déjà installé, rustup peut afficher `cannot install while Rust is installed`, puis `continuing (because the -y flag is set and the error is ignorable)`. Dans la commande ci-dessus, cet avertissement est non bloquant : vérifiez le code de sortie et `cargo --version` après activation de `.cargo/env` ; il doit afficher 1.95.0. Le Rust système reste installé.

Cargo télécharge ses dépendances dans `~/.cargo` (`%USERPROFILE%\.cargo` sous Windows) même si une compilation échoue ; rustup conserve les compilateurs dans `.rustup`. Ces caches sont distincts du préfixe `.local`. Si Cargo n’est pas en 1.95.0 dans ce checkout, vérifiez le PATH (`command -v cargo` sous Bash, `Get-Command cargo` sous PowerShell), `rustup show active-toolchain` et toute surcharge `RUSTUP_TOOLCHAIN`. Arrêtez-vous si `cargo install` échoue : les exemples et la désinstallation ci-dessous nécessitent un binaire effectivement installé.

`exec` conserve le statut du producteur (128 + signal sous Unix). Les routes shells, `--stream` et `--raw-on-failure` conservent leurs flux séparés et le marqueur `[stderr]`.

Vérifiez que l'installation fonctionne partout, sans dépôt :

~~~bash
lm-resizer exec -- echo bonjour
~~~

Les exemples suivants supposent que ce qu'ils enveloppent est présent : `git` et un répertoire courant dans un dépôt Git (hors dépôt, Git échoue lui-même avec le code 128 et il n'y a rien à raccourcir), et un projet Rust avec Cargo pour `cargo test`. Si une commande enveloppée n'est pas installée, `exec` affiche `command not found: <nom>` et sort avec 127.

~~~bash
lm-resizer git log -20
lm-resizer exec --raw-on-failure -- cargo test
lm-resizer tee list
lm-resizer gain --history --project
~~~

Les vues auditées grep/find/listing/fichier/git/conteneur/linter conservent les nombres, chemins, identifiants, auteurs et diagnostics littéraux. Elles ne produisent plus de références `LMR-LINES` ou `LMR-TEXT`. Les réussites Cargo/pytest reconnues peuvent être résumées par les compteurs de suite; les échecs restent visibles. `lm-resizer expand -i vue.txt` reconstruit les vues réversibles, dont les historiques et diffs `Patch v1` : les en-têtes communs et répétitions sont factorisés sans retirer de ligne de source ni de contexte.

Une vue suffisamment réduite peut afficher `[tee:<id>]`. Le rappel se lit avec `lm-resizer tee read <id>` ; sinon `tee list` et le champ JSON `tee_hint` donnent accès au brut sans alourdir la vue.

Depuis Bash, récupérer un original listé lorsqu’il en existe un :

~~~bash
tee_listing=$(lm-resizer tee list)
tee_file=${tee_listing%% *}
if [ -n "$tee_file" ]; then lm-resizer tee read "$tee_file"; fi
~~~

Sous PowerShell, choisir le fichier dans la liste JSON :

~~~powershell
$teeFiles = lm-resizer tee list --json | ConvertFrom-Json
if ($teeFiles.files.Count -gt 0) { lm-resizer tee read $teeFiles.files[0].name }
~~~

Si plusieurs fichiers sont listés, utilisez le nom ou l’identifiant `[raw: …]` correspondant à la commande recherchée.

`lm-resizer --version` affiche `lm-resizer 0.2.4`. Le rappel JSON porte un identifiant tel que `[raw: e3b0c44298fc]` ; les archives ont l’extension `.log`.

Pour retirer un binaire installé avec Cargo :

~~~bash
cargo uninstall --root "$HOME/.local" lm-resizer
~~~

Windows (PowerShell) :

~~~powershell
cargo uninstall --root "$installRoot" lm-resizer
~~~

Le CLI propose aussi `compress` pour les fichiers ou l'entrée standard, `tool-output` pour une sortie déjà capturée, et des intégrations MCP, HTTP et hooks d'agents activées sur demande. `install --client all --scope project` écrit aussi la configuration utilisateur Codex et remplace une table `mcp_servers.lm_resizer` existante sans sauvegarde ; conservez-en une copie avant installation. Voir le [guide des intégrations agents](docs/CLAUDE_CODEX.md) et le [guide de release](docs/RELEASE.md) pour ces usages.

Exemples à copier depuis ce checkout (Bash) :

~~~bash
printf 'hello world\n' | lm-resizer compress
git log -20 '--format=Date: %ad%n%h %s' --date=short | lm-resizer tool-output --command 'git log -20'
~~~

`compress` lit le texte de stdin ; `tool-output` filtre une sortie déjà capturée. Son argument `--command` décrit la commande et ne l’exécute pas. Une petite sortie peut rester intacte.

Pour configurer les quatre clients depuis la racine du projet, après avoir sauvegardé toute configuration Codex existante :

~~~bash
lm-resizer install --client all --scope project
~~~

Cette commande écrit les fichiers du projet et la configuration Codex de votre compte, y compris avec `--scope project`.

## Statistiques de jetons reproductibles

`lm-resizer stats --markdown` affiche les comptes exacts du texte avec le tokenizer existant **tiktoken-rs / o200k_base** (famille GPT-4o). Les JSON `exec`, `tool-output` et `compress` exposent `original_tokens`, `compressed_tokens`, le gain signé `tokens_saved`, `tokenizer` et `token_count_method: "exact"`. Le compte inclut les marqueurs de récupération finaux ; un gain négatif signifie davantage de jetons en sortie. Cet encodage de référence ne mesure ni le tokenizer de Claude/Llama ni une facture fournisseur.

Les nouvelles entrées d’historique conservent les deux comptes. Les statistiques gardent les champs JSON existants d’octets et ajoutent les totaux mesurés et `measured_commands` / `unmeasured_commands`. Les anciennes entrées ne contiennent pas le texte à recompter : leur `estimated_tokens_saved` reste explicitement une **estimation historique octets / 4**, séparée des mesures. `discover`, `discover-sessions`, `eval` et `learn` comptent le texte original et filtré disponible : il s’agit de gains potentiels du filtre. Pour la compatibilité JSON, leur champ `estimated_tokens_saved` est un alias du gain potentiel réellement compté `tokens_saved`.

[Méthode, compatibilité et reproduction](docs/TOKEN-STATISTICS.md). Le banc de parité ci-dessous utilise `o200k_base`.

## Filtres natifs et mesures

Le produit utilise ses propres filtres Rust et TOML. Le chemin normal `exec` archive stdout et stderr entrelacés jusqu’à EOF, sans plafond de 10 Mio. `lm-resizer tee list` et `lm-resizer tee read <id>` retrouvent le brut complet. Les options `--stream` et `--raw-on-failure` conservent une capture séparée des flux.

**Médiane tee compris : 21,74 %, contre 2,17 % avant et 15,81 % pour la référence.** Le format de patch réversible change volontairement les vues de diff ; les 61 bruts et statuts du producteur sont vérifiés. [Mesures actuelles, écarts exacts et limites](bench/native/patch-revision.md).

Les commandes explicites `err`, `test`, `summary`, `json`, `deps`, `env`, `format`, `outline` et `dedup` complètent les filtres. Les lectures de fichiers restent littérales. Les plis réversibles de chemins et correspondances, tables JSON et répétitions exactes complètent les vues de commandes ; le contour syntaxique et la déduplication de blocs sont explicites. [Hooks supplémentaires](docs/AGENT_HOOKS.md) : configuration Gemini, Copilot et Cursor.

## Quand ne pas l'utiliser

- Ne prenez pas une vue raccourcie pour une piste d'audit complète : inspectez l'original conservé pour la sécurité, la conformité ou les échecs subtils.
- N’attendez pas un gain sur toute entrée : les sorties courtes ou peu répétitives peuvent rester intactes.
- Ne déduisez pas des seuls jetons de sortie une baisse de facture API ou de meilleures décisions de l'agent. Ni les tâches d'agent ni la facturation n'ont été testées ici.

LM Resizer est sous licence Apache-2.0. [Contribution](CONTRIBUTING.md) · [Sécurité](SECURITY.md)

[Crédits et licences des inspirations](THIRD-PARTY-NOTICES).
