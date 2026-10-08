# Changelog

## [0.2.6] - préparation du 2026-10-08

La 0.2.5 est publiée. Cette version ajoute le résumé syntaxique local de `smart`, corrige la vue `git log --stat` et documente le rejeu du banc en une commande.

### `smart --ast`
- `lm-resizer smart <fichier> --ast` résume localement (tree-sitter, sans modèle ni réseau) la structure de fichiers Rust, Python, JavaScript et TypeScript : imports, types, signatures et numéro de ligne réel de chaque déclaration, corps omis. Une source invalide ou une extension inconnue donne un repli annoncé en tête (`[smart AST: repli …]`) et la sortie historique de `smart`. Sans `--ast`, la sortie de `smart` reste identique octet pour octet à celle de la 0.2.5 (fixtures `tests/fixtures/smart/`).
- Mesures sur trois fichiers réels du dépôt, en octets : 82,29 %, 93,47 % et 89,08 % de réduction ([`bench/real/smart-ast-measurements.json`](bench/real/smart-ast-measurements.json)). Les jetons du JSON `--ast` restent une estimation octets ÷ 4 (`token_count_method: "approximate"`), contrairement au reste du produit.
- Correction relevée à la relecture : les attributs Rust `#[…]` n'apparaissent plus comme documentation de la déclaration (ils étaient rendus en `// [derive(…)]`), les commentaires s'y lisent au-dessus des attributs ; `#` n'ouvre un commentaire qu'en Python, un champ privé JavaScript `#secret` n'est plus pris pour un commentaire.
- Poids : le binaire release grossit d'environ 12 % (+3,9 Mo mesurés à la relecture) ; dépendances nouvelles tree-sitter (MIT) et streaming-iterator, ajoutées à [`THIRD-PARTY-NOTICES`](THIRD-PARTY-NOTICES).

### `git log --stat`
- La vue montrait le premier commit seulement (mesuré : 1 commit sur 40 et 1 fichier sur 316 visibles, soit 99 % de « réduction » en masquant le reste). Elle affiche désormais chaque commit (en-tête, auteur, date, titre, trois lignes de corps au plus) et chaque fichier avec son nombre de lignes, sans les barres de proportion. Mesuré sur quatre dépôts : 40/40 commits et 316/316 fichiers visibles, réduction de 9 à 58 % en jetons `o200k_base`. Le brut reste dans tee.
- Les autres formes de `git log` ne changent pas, ce qui préserve les chiffres du banc ; leur limite est consignée dans [`docs/KNOWN-MISSES.md`](docs/KNOWN-MISSES.md).

### Banc
- [`bench/real/rejouer.sh`](bench/real/rejouer.sh) rejoue les 61 captures en une étape : Python isolé avec tiktoken, oracle construit depuis l'archive épinglée, binaire du checkout, dossier de rejeu neuf, synthèse (médiane, moyenne, bruts récupérés, codes du producteur). Le README ne renvoie plus au seul script Python du banc, qui exige ses arguments.
- Banc rejoué avec ce script sur le code de cette préparation (corpus de 61 captures, `o200k_base`) : médiane **25,18 %**, moyenne **34,68 %**, brut récupéré 61/61, code du producteur conservé 61/61, vues strictement égales à l'oracle 41/61 — chiffres identiques à ceux de la 0.2.5, vue par vue (aucun des 61 cas n'a changé). Le même script, lancé sur une copie neuve de l'arbre, redonne ces chiffres.

## [0.2.5] - publication prévue le 2026-10-08

La version 0.2.4 n'a jamais été étiquetée ni publiée : son contenu (section suivante) est livré avec 0.2.5, qui est la première version publiée depuis 0.2.3.

### Vues génériques et statistiques
- `lm-resizer err|test|summary -- <commande>` garde, pour n'importe quel producteur, les diagnostics ou les bilans de tests avec le contexte voisin, conserve le code de sortie et archive la sortie complète (`tee read`). `exec` et `tool-output` résument aussi les commandes sans filtre dédié ; l'outil MCP `lm_resizer_tool_output` marque un statut non nul `isError: true` ([`docs/CLI-REFERENCE.md`](docs/CLI-REFERENCE.md)).
- `gain` compte toutes les commandes passées par `exec`/`tool-output` (jetons mesurés `o200k_base`, `--json`, `--history`, `--project`).
- Mesure rejouée sur le corpus de 61 captures : médiane **25,18 %** (RTK 0.50.0 : 15,81 %), moyenne **34,68 %** (RTK : 32,61 %), brut et code du producteur conservés 61/61, vues strictement égales à RTK 41/61 après correction du découpage du banc. Les bilans de test en échec n'ajoutent plus d'en-tête redondant ; les assertions pytest, les messages TypeScript et les blocs d'échec Cargo restent entiers. Les sorties vers un tube fermé se terminent silencieusement avec le code 141 sous Unix. Détail : [`bench/native/windows-release/delivery.md`](bench/native/windows-release/delivery.md).

### Hooks et agents
- `install-hooks` et `init` sont idempotents : un contenu identique réussit sans réécriture, un contenu divergent est refusé sans `--force`. `uninstall-hooks` retire aussi les helpers générés et ne retire une configuration native que si elle est encore exactement le JSON généré. Réserve : les helpers sont supprimés sans comparer leur contenu ([`tests/hooks_idempotence.rs`](tests/hooks_idempotence.rs)).
- `rewrite-shell` partage le refus du hook PreToolUse : redirection, tube, substitution, here-doc et commande interactive restent la ligne d'origine ; `&&`, `||` et `;` sont toujours réécrits segment par segment.

### Documentation
- README FR/EN réécrits (accroche, installation en tête, comparaison sourcée avec RTK 0.50.0 et Headroom 0.39.1, commande de rejeu), référence CLI complète ([`docs/CLI-REFERENCE.md`](docs/CLI-REFERENCE.md)) et contrôles de cohérence ([`scripts/check-cli-reference.sh`](scripts/check-cli-reference.sh), [`scripts/check-readme-parity.cjs`](scripts/check-readme-parity.cjs), [`tests/cli_doc_alignment.rs`](tests/cli_doc_alignment.rs)).
- Précisions : `cargo install` refuse si `~/.local/bin/lm-resizer` existe déjà ; message exact de `exec` pour une commande absente ; les médianes sont celles du corpus de 61 captures, pas une promesse sur des dépôts arbitraires ; `install --client all` écrit toujours la configuration Codex de l'utilisateur.

### Distribution
- Les archives de release incluent désormais `docs/CLI-REFERENCE.md`, `docs/AGENT_HOOKS.md` et `docs/WINDOWS.md`, liés depuis le README ([`scripts/package-release.sh`](scripts/package-release.sh), [`scripts/package-release.ps1`](scripts/package-release.ps1)). Les liens vers `docs/RELEASE.md` et `bench/` restent des liens du dépôt, absents de l'archive.
- La garde de release télécharge désormais l'archive source RTK épinglée sur un clone neuf et refuse un SHA-256 différent ; l'absence de réseau produit une erreur explicite.

### Limites connues
- `exec` conserve son état dans `~/lm-resizer` (archives de rappel, base CCR, historique) : ne clonez pas le dépôt directement dans votre dossier personnel.
- Les médianes du README sont celles du corpus de 61 captures, pas une promesse sur des dépôts arbitraires.

## [0.2.4] - jamais publiée, incluse dans 0.2.5

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
