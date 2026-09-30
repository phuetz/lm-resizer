# LM Resizer

**Raccourcir les sorties de commande bruyantes avant qu'elles n'arrivent à un agent de code.** LM Resizer est un outil Rust local qui peut lancer une commande, donner un résultat compact à l'agent et conserver la sortie originale pour inspection. Il traite les journaux de tests, les sorties Git, les diagnostics de compilation, le JSON et d'autres textes ; le résultat dépend de l'entrée et de la commande.

![Exemple de traitement d'une sortie de commande par LM Resizer](docs/lm-resizer-hero.png)

[English](README.md) · [Site](https://phuetz.github.io/lm-resizer/) · [Méthode du banc et ses 22 cas](bench/README.md) · [FAQ](docs/FAQ.fr.md) · [Pertes connues](docs/KNOWN-MISSES.fr.md)

## Installer et essayer

### Binaire précompilé (v0.2.3)

La release binaire v0.2.3 est publiée. Utilisez la commande de votre plateforme. L'installeur vérifie la somme SHA-256 de l'archive et la version du binaire avant de poser `lm-resizer` dans `~/.local/bin` par défaut. Plateformes disponibles : Linux x86_64, macOS x86_64/arm64 et Windows x86_64. La release v0.2.2 existante n'a pas d'archives précompilées.

Linux et macOS :

~~~sh
bash -o pipefail -c 'curl -fsSL https://raw.githubusercontent.com/phuetz/lm-resizer/v0.2.3/install.sh | sh'
~~~

Windows PowerShell :

~~~powershell
irm https://raw.githubusercontent.com/phuetz/lm-resizer/v0.2.3/install.ps1 | iex
~~~

Sur Linux/macOS, ajoutez `~/.local/bin` au `PATH` si l'installeur le demande. L'installeur Windows met à jour le `PATH` utilisateur. `LM_RESIZER_INSTALL_DIR` permet de choisir un autre répertoire. Pour désinstaller un binaire précompilé, retirez-le de ce répertoire.

### Compiler depuis les sources

Il faut Rust/Cargo (Rust 1.86 ou plus récent). Depuis un checkout de ce dépôt :

~~~bash
cargo install --quiet --path . --locked --root "$HOME/.local"
export PATH="$HOME/.local/bin:$PATH"
lm-resizer --version
~~~

Essayez l'outil sur une **vraie sortie** de ce checkout. `exec` lance la commande puis affiche la sortie traitée. Le code de sortie est conservé ; pour une commande en échec, utilisez `--raw-on-failure` si vous avez besoin immédiatement de toute la sortie d'erreur.

~~~bash
lm-resizer exec -- git log -20 '--format=Date: %ad%n%h %s' --date=short
lm-resizer tee list
~~~

La première commande lit 20 vrais commits Git et retire leurs lignes de date de la vue de l'agent. Quand la sortie est raccourcie, `exec` peut afficher un identifiant `[raw: …]`. Donnez cet identifiant à `tee read` pour retrouver le texte original. Les fichiers bruts et la base CCR restent locaux ; conservez-les si vous aurez besoin des preuves plus tard. La sortie peut aussi rester intacte si la compression n'apporte rien.

Pour retirer un binaire installé avec Cargo :

~~~bash
cargo uninstall --root "$HOME/.local" lm-resizer
~~~

Le CLI propose aussi `compress` pour les fichiers ou l'entrée standard, `tool-output` pour une sortie déjà capturée, et des intégrations MCP, HTTP et hooks d'agents activées sur demande. Voir le [guide des intégrations agents](docs/CLAUDE_CODEX.md) et le [guide de release](docs/RELEASE.md) pour ces usages.

## Mesures face à RTK et Headroom

**Rejeu du 30/09/2026** depuis le commit de fusion `df30334`, sur Linux x86_64, 24 cœurs logiques et 93 Gio de RAM. La comparaison utilise RTK 0.50.0, Headroom 0.39.1 avec ONNX Runtime 1.24.4, et les mêmes 22 fixtures. `o200k_base` compte les jetons de sortie. Une économie n'est retenue que si tous les faits de l'oracle déclaré pour le cas sont conservés ; sinon, l'*économie qualifiée* vaut zéro. Trois fixtures viennent de vrais outils ; les autres sont synthétiques. [Méthode, fixtures et résultats détaillés](bench/README.md).

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
| `git_diff` | 195 | **47 %** (103 jetons) | 36 % (125 jetons) | 0 % | LM Resizer économise davantage. |
| `compile_error` | 106 | 0 % | **26 %** | 0 % | RTK économise davantage. |
| Six cas de code source | 379–481 chacun | 0 % | 0 % | 0 % | Aucun gain mesuré. |

Sur sept cas, les réductions brutes de RTK omettent au moins un fait exigé par l'oracle : leur économie qualifiée vaut donc zéro. Ces fixtures ne mesurent ni la facture des fournisseurs, ni la réussite de tâches par un agent de code, ni les performances sur toute sortie réelle. La latence dépend de la machine et du cache. Voir les [données par cas](bench/resultats.json) et les [pertes restantes](docs/KNOWN-MISSES.fr.md).

## Quand ne pas l'utiliser

- Ne prenez pas une vue raccourcie pour une piste d'audit complète : inspectez l'original conservé pour la sécurité, la conformité ou les échecs subtils.
- N'attendez pas un gain sur chaque entrée. Les six cas de code source ci-dessus restent inchangés, et RTK dépasse LM Resizer dans deux cas mesurés (`dotnet_ok` et `compile_error`).
- Ne déduisez pas des seuls jetons de sortie une baisse de facture API ou de meilleures décisions de l'agent. Ni les tâches d'agent ni la facturation n'ont été testées ici.

LM Resizer est sous licence Apache-2.0. [Contribution](CONTRIBUTING.md) · [Sécurité](SECURITY.md)
