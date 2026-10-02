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

Linux/macOS (Bash) :

~~~bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path --profile minimal --default-toolchain 1.86.0
. "$HOME/.cargo/env"
git clone --branch release/v0.2.4-recette-2026-10-01 https://github.com/phuetz/lm-resizer.git
cd lm-resizer
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
git clone --branch release/v0.2.4-recette-2026-10-01 https://github.com/phuetz/lm-resizer.git
cd lm-resizer
cargo --version
$installRoot = Join-Path $env:USERPROFILE '.local'
cargo install --quiet --path . --locked --root "$installRoot"
$env:Path = "$installRoot\bin;$env:Path"
lm-resizer --version
~~~

Cargo télécharge ses dépendances dans `~/.cargo` (`%USERPROFILE%\.cargo` sous Windows) même si une compilation échoue ; rustup conserve les compilateurs dans `.rustup`. Ces caches sont distincts du préfixe `.local`. Si Cargo n’est pas en 1.86.0 dans ce checkout, vérifiez le PATH (`command -v cargo` sous Bash, `Get-Command cargo` sous PowerShell), `rustup show active-toolchain` et toute surcharge `RUSTUP_TOOLCHAIN`. Arrêtez-vous si `cargo install` échoue : les exemples et la désinstallation ci-dessous nécessitent un binaire effectivement installé.

Essayez l'outil sur une **vraie sortie** de ce checkout. `exec` lance la commande puis affiche la sortie traitée. Les codes de sortie normaux sont conservés. Un processus terminé par un signal ou une commande impossible à lancer renvoie 1 ; pour une commande en échec, utilisez `--raw-on-failure` si vous avez besoin immédiatement de toute la sortie d'erreur.

~~~bash
lm-resizer exec -- git log -20 '--format=Date: %ad%n%h %s' --date=short
lm-resizer tee list
~~~

Extraits de sorties réellement capturées sur Debian 13 avec Rust 1.86.0, au commit `86bf58f` (les SHA, noms de fichiers et chemins varieront sur votre machine) :

`lm-resizer --version` :

~~~text
lm-resizer 0.2.4
~~~

`exec` : trois premières lignes, résumé des lignes retirées et marqueur final :

~~~text
86bf58f Livrer la documentation des jetons dans les archives binaires
4694506 Corriger les avertissements Clippy dans les tests du workspace
5a12cfb Éviter le débordement de pile du CLI de développement sous Windows
... omitted 20 low-signal lines
[raw: e6c2d124d10b]
~~~

`lm-resizer tee list` :

~~~text
e6c2d124d10b5924173b779b04b40df78dce01a9678d825a706afa3bd3ad556a.log 1786 bytes /qa/debian-home/.local/state/lm-resizer/tee/e6c2d124d10b5924173b779b04b40df78dce01a9678d825a706afa3bd3ad556a.log
~~~

Un petit résultat peut rester inchangé, sans gain ni marqueur de récupération :

~~~bash
lm-resizer exec -- git rev-parse --short HEAD
~~~

~~~text
86bf58f
~~~

La première commande lit 20 vrais commits Git et retire leurs lignes de date de la vue de l'agent. Quand la sortie est raccourcie, `exec` peut afficher un identifiant `[raw: …]`. Donnez cet identifiant à `tee read` pour retrouver le texte original. La récupération concerne le texte UTF-8 : les octets non UTF-8 sont remplacés au décodage, et stdout/stderr sont combinés. Les fichiers tee restent locaux jusqu’à leur suppression ou purge. Les entrées CCR expirent après **30 minutes par défaut**, même si la base est conservée ; récupérez-les et exportez-les avant expiration pour garder une preuve durable. La sortie peut aussi rester intacte si la compression n'apporte rien.

Pour retirer un binaire installé avec Cargo :

~~~bash
cargo uninstall --root "$HOME/.local" lm-resizer
~~~

Windows (PowerShell) :

~~~powershell
cargo uninstall --root "$installRoot" lm-resizer
~~~

Le CLI propose aussi `compress` pour les fichiers ou l'entrée standard, `tool-output` pour une sortie déjà capturée, et des intégrations MCP, HTTP et hooks d'agents activées sur demande. `install --client all --scope project` écrit aussi la configuration utilisateur Codex et remplace une table `mcp_servers.lm_resizer` existante sans sauvegarde ; conservez-en une copie avant installation. Voir le [guide des intégrations agents](docs/CLAUDE_CODEX.md) et le [guide de release](docs/RELEASE.md) pour ces usages.

## Statistiques de jetons reproductibles

`lm-resizer stats --markdown` affiche les comptes exacts du texte avec le tokenizer existant **tiktoken-rs / o200k_base** (famille GPT-4o). Les JSON `exec`, `tool-output` et `compress` exposent `original_tokens`, `compressed_tokens`, le gain signé `tokens_saved`, `tokenizer` et `token_count_method: "exact"`. Le compte inclut les marqueurs de récupération finaux ; un gain négatif signifie davantage de jetons en sortie. Cet encodage de référence ne mesure ni le tokenizer de Claude/Llama ni une facture fournisseur.

Les nouvelles entrées d’historique conservent les deux comptes. Les statistiques gardent les champs JSON existants d’octets et ajoutent les totaux mesurés et `measured_commands` / `unmeasured_commands`. Les anciennes entrées ne contiennent pas le texte à recompter : leur `estimated_tokens_saved` reste explicitement une **estimation historique octets / 4**, séparée des mesures. `discover`, `discover-sessions`, `eval` et `learn` comptent le texte original et filtré disponible : il s’agit de gains potentiels du filtre. Pour la compatibilité JSON, leur champ `estimated_tokens_saved` est un alias du gain potentiel réellement compté `tokens_saved`.

[Méthode, compatibilité et reproduction](docs/TOKEN-STATISTICS.md). Le banc ci-dessous utilisait déjà `o200k_base` ; ses résultats historiques sur fixtures sont conservés.

## Mesures face à RTK et Headroom

**Rejeu du 30/09/2026** depuis le commit de fusion `df30334`, sur Linux x86_64. Le JSON de mesure versionné ne consigne ni CPU ni RAM. La comparaison utilise RTK 0.50.0, Headroom 0.39.1 avec ONNX Runtime 1.24.4, et les mêmes 22 fixtures. `o200k_base` compte les jetons de sortie. Une économie n'est retenue que si tous les faits de l'oracle déclaré pour le cas sont conservés ; sinon, l'*économie qualifiée* vaut zéro. Trois fixtures viennent de vrais outils ; les autres sont synthétiques. [Méthode, fixtures et résultats détaillés](bench/README.md).

| Résultat sur 22 cas | LM Resizer | RTK | Headroom |
|---|---:|---:|---:|
| Victoires seules sur l'économie qualifiée | 13 | 2 | 0 |
| Victoire partagée | 1 avec RTK | 1 avec LM Resizer | 0 |
| Médiane de l'économie qualifiée entre cas | 74,7 % | 0,0 % | 0,0 % |
| Oracle déclaré complet | 22/22 | 15/22 | 22/22 |

Six autres cas n'ont **aucun gain qualifié, quel que soit l'outil**. Ces exemples donnent le nombre mesuré de jetons en entrée et l'économie qualifiée ; zéro peut signifier une sortie inchangée ou un oracle incomplet.

| Cas | Jetons d'entrée | LM Resizer | RTK | Headroom | Constat |
|---|---:|---:|---:|---:|---|
| `cargo_ok` | 837 | 97 % | 97 % | 0 % | Égalité entre LM Resizer et RTK. |
| `logs` | 2 009 | 96 % | 95 % | 92 % | Les trois conservent l'oracle déclaré. |
| `dotnet_ok` | 111 | 50 % | **81 %** | 0 % | RTK économise davantage. |
| `git_diff` | 195 | **47 %** (103 jetons restants) | 36 % (125 jetons restants) | 0 % | LM Resizer économise davantage. |
| `compile_error` | 106 | 0 % | **26 %** | 0 % | RTK économise davantage. |
| Six cas de code source | 379–481 chacun | 0 % | 0 % | 0 % | Aucun gain mesuré. |

Sur sept cas, les réductions brutes de RTK omettent au moins un fait exigé par l'oracle : leur économie qualifiée vaut donc zéro. Ces fixtures ne mesurent ni la facture des fournisseurs, ni la réussite de tâches par un agent de code, ni les performances sur toute sortie réelle. La latence dépend de la machine et du cache. Voir les [données par cas](bench/resultats.json) et les [pertes restantes](docs/KNOWN-MISSES.fr.md).

## Quand ne pas l'utiliser

- Ne prenez pas une vue raccourcie pour une piste d'audit complète : inspectez l'original conservé pour la sécurité, la conformité ou les échecs subtils.
- N'attendez pas un gain sur chaque entrée. Les six cas de code source ci-dessus restent inchangés, et RTK dépasse LM Resizer dans deux cas mesurés (`dotnet_ok` et `compile_error`).
- Ne déduisez pas des seuls jetons de sortie une baisse de facture API ou de meilleures décisions de l'agent. Ni les tâches d'agent ni la facturation n'ont été testées ici.

LM Resizer est sous licence Apache-2.0. [Contribution](CONTRIBUTING.md) · [Sécurité](SECURITY.md)
