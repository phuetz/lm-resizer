# Changelog

## [0.2.4] - publication prévue le 2026-10-08

### Installation et distribution
- Versions Cargo, npm, WASM, plugin et installateurs alignées ; contrôle centralisé dans [`scripts/check-release-version.cjs`](scripts/check-release-version.cjs).
- Recette source corrigée après le test Debian : rustup officiel, clone explicite, prérequis C/C++ et parcours PowerShell. Rust minimal **1.91 pour le CLI** (core/wasm : 1.86 ; toolchain épinglée : 1.95.0) ([`Cargo.toml`](Cargo.toml), [`rust-toolchain.toml`](rust-toolchain.toml)) ; le Rust système trop ancien doit être remplacé par la toolchain indiquée.
- Archives allégées depuis v0.2.3, comprenant la documentation liée depuis les README et les skills utilisateur ([`scripts/package-release.sh`](scripts/package-release.sh), [`scripts/package-release.ps1`](scripts/package-release.ps1)).
- Installation POSIX : fichiers SHA-256 sans saut de ligne final et avec CRLF acceptés ; fichier vide, archive altérée, nom incorrect et mauvaise version restent rejetés ([`install.sh`](install.sh), [`scripts/test-install-sha-format.sh`](scripts/test-install-sha-format.sh), [`scripts/test-install-binary.sh`](scripts/test-install-binary.sh)).
- Les builds de paquetage natif et WASM remplacent les chemins personnels du constructeur par des préfixes neutres, y compris pour les dépendances Rust ([`scripts/build-release-artifact.cjs`](scripts/build-release-artifact.cjs), [`scripts/test-release-paths.cjs`](scripts/test-release-paths.cjs)).
- La garde de release teste désormais tout le workspace ; les métadonnées de preuve reflètent cette commande ([`scripts/check-release.sh`](scripts/check-release.sh), [`scripts/check-release.ps1`](scripts/check-release.ps1), [`scripts/release-evidence.sh`](scripts/release-evidence.sh)).

### Récupération et diagnostics
- La première clé CCR du CLI conserve l'entrée UTF-8 exacte avant transformation (`60d5cc6`, absent de v0.2.3) ; récupération avant expiration, **30 minutes** par défaut ([`tests/cli_retrieval.rs`](tests/cli_retrieval.rs), [`DEFAULT_TTL = 1800 s`](crates/lm-resizer-core/src/ccr/mod.rs)). Les scripts shell générés deviennent exécutables et `discover` rejette un chemin explicitement demandé mais absent.
- Récupération CCR ajoutée à l'ABI C et au wrapper JavaScript WASM ; magasins partagés entre appels, limités à **1000 entrées**. Le magasin natif expire ; le magasin WASM n'applique pas de TTL ([`docs/ABI.md`](docs/ABI.md), [`packages/wasm/README.md`](packages/wasm/README.md), [`DEFAULT_CAPACITY`](crates/lm-resizer-core/src/ccr/mod.rs)). L'éviction ne supprime plus une clé fraîchement réinsérée.
- Le store des transmissions partagées utilise WAL et `synchronous=FULL` ; clés invalides et doublons ont des diagnostics explicites ([`src/shared_context.rs`](src/shared_context.rs)).
- Erreurs Python conservées par la porte diagnostique ; un compte d'échecs finissant par zéro reste un échec ; horodatage TRX non ASCII rejeté sans panique ; fichier d'advice illisible ou invalide nommé dans l'erreur ([`diagnostic_gate.rs`](crates/lm-resizer-core/src/transforms/diagnostic_gate.rs), [`output.rs`](crates/lm-resizer-core/src/output.rs), [`parity_filters.rs`](src/parity_filters.rs), [`advice_cli.rs`](src/advice_cli.rs)).

### CLI et documentation
- `exec` transmet stdin au processus enfant. Arguments vides de la réécriture shell conservés. Filtres : fichiers Git commençant par `use`, chemins TypeScript avec parenthèses, contexte de recherche et noms datés, assertion Pytest fautive conservés ; progression Curl retirée ([`src/main.rs`](src/main.rs), [`src/command_filters.rs`](src/command_filters.rs), [`tests/exec_stdin.rs`](tests/exec_stdin.rs)).
- `rewrite-shell` ne réécrit plus une commande dont la sortie est redirigée (`>`, `>>`, `2>`, `&>`), tubée (`|`, `| tee`, `|&`), captée par `$(...)` ou des backticks, fournie par un here-doc, ou interactive. Recoller le suffixe sur `lm-resizer exec` écrivait la vue réduite dans le fichier. Le refus est celui du hook PreToolUse ; `&&`, `||` et `;` restent réécrits segment par segment ([`src/main.rs`](src/main.rs), [`docs/CLAUDE_CODEX.md`](docs/CLAUDE_CODEX.md)).
- Démarrage CLI Windows : thread joint avec une pile explicite de **8 Mio**, couvrant le débordement de pile du parseur de développement ([`main`](src/main.rs), [`tests/cli_stack.rs`](tests/cli_stack.rs)).
- Statistiques CLI mesurées avec `tiktoken-rs/o200k_base`, séparées des estimations historiques octets/4 ; aucune promesse de coût fournisseur ([`docs/TOKEN-STATISTICS.md`](docs/TOKEN-STATISTICS.md)).
- Skill Grok corrigé pour `--version` et les outils MCP. Banc décrit honnêtement : **22 fixtures, 3 captures réelles, 19 synthétiques, 66 mesures** ; résultats historiques, pas une nouvelle mesure de cette release ([`bench/cases.json`](bench/cases.json), [`bench/src/main.rs`](bench/src/main.rs), [`bench/resultats.json`](bench/resultats.json), [`bench/RAPPORT.md`](bench/RAPPORT.md)). Aucun gain hors de ce corpus n'est affirmé.
- Documentation des limites CCR, UTF-8, codes de sortie et configuration corrigée ; comparaisons non sourcées retirées. Les preuves et sommes de contrôle locales sont régénérées, sans être versionnées.

### Non retenu
- Purge automatique tee : tests instables et suppression silencieuse des journaux ; à reprendre pour une version ultérieure.
- Modification du filtre Git log : test déjà passant sans correctif et suppression de lignes valides ; à reprendre pour une version ultérieure.

## [0.2.3] - publiée avant cette préparation

### Added
- Installateurs binaires en une commande pour Linux, macOS et Windows, avec vérification SHA-256 des archives de release.
- Test CI de l'installation Linux sur un runner neuf sans Rust et préparation d'une release avec archives.

## [0.2.2] - 2026-09-16

### Added
- Skill Grok « lm-resizer » (`skills/grok/lm-resizer/`) : déclenchement contextuel quand une tâche bénéficie d'une compression locale (`compress -i`), sans envelopper chaque commande.
- Installateur idempotent `scripts/install-grok-skill.sh` (`--target grok|codex`, `--dest DIR`, `--force` avec sauvegarde horodatée) : préserve les fichiers personnalisés, répare un fichier manquant, refuse les arguments inconnus (exit 2) ; tests `scripts/test-install-grok-skill.sh`.
- Auteur des skills et de l'installateur : Grok 4.6 (revue croisée Claude Opus 5, corrections appliquées).

## [0.2.1] - antérieur
- Voir l'historique git.
