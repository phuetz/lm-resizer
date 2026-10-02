# LM Resizer

**Raccourcir les sorties de commande bruyantes avant qu'elles n'arrivent à un agent de code.** LM Resizer est un outil Rust local qui peut lancer une commande, donner un résultat compact à l'agent et conserver la sortie originale pour inspection. Il traite les journaux de tests, les sorties Git, les diagnostics de compilation, le JSON et d'autres textes ; le résultat dépend de l'entrée et de la commande.

![Exemple de traitement d'une sortie de commande par LM Resizer](docs/lm-resizer-hero.png)

[English](README.md) · [Site](https://phuetz.github.io/lm-resizer/) · [Méthode du banc et ses 22 cas](bench/README.md) · [FAQ](docs/FAQ.fr.md) · [Pertes connues](docs/KNOWN-MISSES.fr.md)

## Installer et essayer

### Binaire précompilé, après la publication de v0.2.4

**Ne lancez pas les commandes ci-dessous avant la publication de la release v0.2.4 et de ses archives : elles renvoient actuellement HTTP 404. Pour installer maintenant, suivez « Compiler depuis les sources » plus bas.**

Après cette publication seulement, utilisez la commande de votre plateforme. L'installeur vérifie la somme SHA-256 de l'archive et la version du binaire avant de poser `lm-resizer` dans `~/.local/bin` par défaut. Plateformes préparées : Linux x86_64, macOS x86_64/arm64 et Windows x86_64. La release v0.2.2 existante n'a pas d'archives précompilées.

Linux et macOS :

~~~sh
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/phuetz/lm-resizer/v0.2.4/install.sh | sh'
~~~

Windows PowerShell :

~~~powershell
irm https://raw.githubusercontent.com/phuetz/lm-resizer/v0.2.4/install.ps1 | iex
~~~

Sur Linux/macOS, ajoutez `~/.local/bin` au `PATH` si l'installeur le demande. L'installeur Windows met à jour le `PATH` utilisateur. `LM_RESIZER_INSTALL_DIR` permet de choisir un autre répertoire. Pour désinstaller un binaire précompilé, retirez-le de ce répertoire.

### Compiler depuis les sources

Rust **1.86.0 ou plus récent** est requis. Le Rust 1.85.1 fourni par Debian 13 est trop ancien : utilisez **rustup officiel**, pas seulement le Cargo du système. Le fichier `rust-toolchain.toml` sélectionne 1.86.0 dans ce dépôt via les commandes rustup ; il inclut `rustfmt` et `clippy` pour les contrôles de développement. Un `cargo +stable` explicite ou `RUSTUP_TOOLCHAIN` peut remplacer ce choix.

Prérequis système à installer avant Rust :

- **Debian 13 / Ubuntu** : `curl`, certificats HTTPS, Git, compilateur C (`cc`/`gcc`), compilateur C++ (`c++`/`g++`), en-têtes de la libc et éditeur de liens (`binutils`, installé avec GCC). Dans un terminal avec sudo (ou comme root sans `sudo`) :

~~~bash
sudo apt-get update
sudo apt-get install -y --no-install-recommends ca-certificates curl git gcc g++ libc6-dev
~~~

- **macOS** : installez les outils de ligne de commande Xcode (`xcode-select --install`) ; ils fournissent Git, Clang/Clang++, les en-têtes et le SDK. Terminez la boîte de dialogue d’installation avant de poursuivre.
- **Windows x86_64** : installez [Git for Windows](https://git-scm.com/downloads/win) et [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022) avec **Développement Desktop en C++**, les outils **MSVC C++ x64/x86** et le **SDK Windows**. Si ces composants sont déjà présents, conservez-les.

La compilation native par défaut a été vérifiée sur Debian 13 avec Rust 1.86.0 **sans `make`, `pkg-config` ni `cmake`**. GCC seul ne suffit pas : `esaxx-rs` compile du C++. SQLite et Oniguruma sont intégrés ; leurs paquets de développement système ne sont pas requis. Les fonctions optionnelles, notamment `--features magika`, ne font pas partie de cette recette.

Si Rust manque ou est trop ancien, exécutez les étapes d’installation de rustup au début du bloc de votre plateforme. Si rustup est déjà installé et son dossier bin dans le PATH, vous pouvez les omettre : il installera la toolchain indiquée par le dépôt. L’installeur ci-dessous sélectionne 1.86.0 par défaut dans votre compte ; `--no-modify-path` laisse vos fichiers de profil et le PATH permanent intacts. Les commandes suivantes règlent seulement le PATH du terminal courant. [Instructions officielles Rust](https://www.rust-lang.org/tools/install/).

Avant publication, partez d’un checkout de la candidate `release/v0.2.4-preparation-2026-10-02` fourni par le mainteneur. Cette branche locale n’est pas nécessairement disponible sur GitHub. **Après publication seulement**, téléchargez les sources du tag public (Bash ou PowerShell) :

~~~sh
git clone --branch v0.2.4 https://github.com/phuetz/lm-resizer.git
cd lm-resizer
~~~

Les blocs Rust ci-dessous s’exécutent depuis la racine de ce checkout. Si vous êtes déjà dans un checkout, ne le clonez pas une deuxième fois.

Linux/macOS (Bash) :

~~~bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path --profile minimal --default-toolchain 1.86.0
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
& $rustupInstaller -y --no-modify-path --profile minimal --default-toolchain 1.86.0
$env:Path = "$(Join-Path $env:USERPROFILE '.cargo\bin');$env:Path"
cargo --version
$installRoot = Join-Path $env:USERPROFILE '.local'
cargo install --quiet --path . --locked --root "$installRoot"
$env:Path = "$installRoot\bin;$env:Path"
lm-resizer --version
~~~

Avec le Rust de la distribution déjà installé, rustup peut afficher `cannot install while Rust is installed`, puis `continuing (because the -y flag is set and the error is ignorable)`. Dans la commande ci-dessus, cet avertissement est non bloquant : vérifiez le code de sortie et `cargo --version` après activation de `.cargo/env` ; il doit afficher 1.86.0. Le Rust système reste installé.

Cargo télécharge ses dépendances dans `~/.cargo` (`%USERPROFILE%\.cargo` sous Windows) même si une compilation échoue ; rustup conserve les compilateurs dans `.rustup`. Ces caches sont distincts du préfixe `.local`. Si Cargo n’est pas en 1.86.0 dans ce checkout, vérifiez le PATH (`command -v cargo` sous Bash, `Get-Command cargo` sous PowerShell), `rustup show active-toolchain` et toute surcharge `RUSTUP_TOOLCHAIN`. Arrêtez-vous si `cargo install` échoue : les exemples et la désinstallation ci-dessous nécessitent un binaire effectivement installé.

Essayez sur les sorties réelles de ce dépôt. `exec` conserve le code de sortie de la commande; sous Unix, un signal donne `128 + signal`. stdout et stderr sont capturés séparément, réunis avec un marqueur `[stderr]` protégé, et leurs tailles sont indiquées dans le JSON. L’ordre chronologique entre les deux flux n’est pas reconstruit.

~~~bash
lm-resizer git log -20
lm-resizer exec --raw-on-failure -- cargo test
lm-resizer tee list
lm-resizer gain --history --project
~~~

Les sorties répétitives peuvent utiliser `LMR-LINES/2` : les préfixes et lignes déjà vues sont référencés sans troncature. Dates Git, hash complets et lignes de diff restent reconstructibles. Les réussites individuelles des tests peuvent être résumées par les compteurs de suite; les diagnostics restent complets. Pour restaurer une vue enregistrée sans la commande originale :

~~~bash
lm-resizer expand -i vue.txt
~~~

Une vue réduite contient un identifiant `[raw: …]`, utilisable avec `tee read`. Une petite sortie peut rester inchangée et ne créer aucune entrée tee. La récupération concerne actuellement le texte UTF-8 : les octets non UTF-8 sont remplacés au décodage. Les fichiers tee restent locaux jusqu’à leur suppression ou purge; les entrées CCR expirent après **30 minutes par défaut**. Exportez les preuves avant expiration pour les conserver durablement.

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

[Méthode, compatibilité et reproduction](docs/TOKEN-STATISTICS.md). Le banc réel ci-dessous utilise `o200k_base` et `cl100k_base`.

## Mesures sur des commandes réelles

**02/10/2026, Linux x86_64** : 30 commandes sur ripgrep, FastAPI et TypeScript épinglés par commit, puis cinq cas supplémentaires. Python tiktoken 0.14.0 compte les sorties réelles avec les deux encodages. Les moyennes et médianes sont calculées par commande, marqueurs compris, sans pondération. Les économies RTK restent comptées même lorsque son oracle échoue.

| Encodage / statistique | LMR avant | LMR après | RTK 0.50.0 |
|---|---:|---:|---:|
| cl100k médiane | 0,00 % | 3,74 % | 43,28 % |
| cl100k moyenne | 7,94 % | 13,21 % | 47,33 % |
| o200k médiane | 0,00 % | 3,75 % | 43,21 % |
| o200k moyenne | 7,96 % | 13,26 % | 47,41 % |

LMR conserve **160 014 faits déclarés sur 160 014**, les 35 codes de sortie et les 24 originaux modifiés récupérés à l’octet près depuis tee. Les vues avant correction perdaient 359 faits selon l’oracle renforcé, qui contrôle aussi leur multiplicité. La régression grep remonte à `bb85e73`; sa correction utilise des vues réversibles plutôt que l’ancien plafond de lignes.

**RTK économise encore beaucoup plus en médiane.** Sur ce corpus, sa vue pytest omet le compteur `457 errors` et son listing TypeScript récursif omet `lib.dom.d.ts` et `checker.ts`. LMR garde ces faits. Leur conservation coûte des jetons : la parité fonctionnelle et une médiane comparable ne sont **pas atteintes**.

Sur le listing TypeScript épinglé (2,65 Mo), le CLI intégré prend **0,191 s en médiane**, au maximum **0,200 s**, sur cinq processus froids. Le rejeu original prenait **43,6 s** sur cette machine. Les six cas de gros volumes passent leurs seuils stricts; ces mesures ne garantissent pas les délais sur d’autres machines.

[Reproduction et limites](bench/real/README.md) · [Tableau avant/après complet](bench/real/RESULTATS.md) · [Inventaire et écarts restants](bench/real/PARITE-RTK.md) · [Banc de performance](bench/perf/README.md).

L’ancien classement surtout synthétique en « économies qualifiées » reste une [archive historique](bench/README.md), pas un chiffre d’annonce actuel ni une promesse générale. Le banc réel comprend des commandes de test en échec et des dépendances Python incomplètes. RTK utilise `pipe` sur les captures identiques lorsque disponible, sinon des commandes relancées; les modes sont indiqués. Son oracle strict peut aussi échouer sur une reformulation non reconnue : son compteur n’est pas présenté comme une mesure universelle de perte sémantique.

## Quand ne pas l'utiliser

- Ne prenez pas une vue raccourcie pour une piste d'audit complète : inspectez l'original conservé pour la sécurité, la conformité ou les échecs subtils.
- N’attendez pas un gain sur toute entrée : les sorties courtes ou peu répétitives peuvent rester intactes.
- Ne déduisez pas des seuls jetons de sortie une baisse de facture API ou de meilleures décisions de l'agent. Ni les tâches d'agent ni la facturation n'ont été testées ici.

LM Resizer est sous licence Apache-2.0. [Contribution](CONTRIBUTING.md) · [Sécurité](SECURITY.md)
