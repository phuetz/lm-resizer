# Changelog

## [0.2.6] - préparation du 2026-10-08

La 0.2.5 est publiée. Cette version corrige deux failles de sécurité relevées par un audit, ajoute le résumé syntaxique local de `smart`, corrige les vues `git log` et pytest, retire les chemins du constructeur du binaire et documente le rejeu du banc en une commande.

### Capture des commandes
- `--stream` et `--raw-on-failure` conservent la provenance des flux dans la vue et dans le champ JSON `streams` : stdout est affiché avant une frontière `[stderr]`, avec une annotation quand les deux flux sont présents. Cette disposition ne représente pas leur chronologie. Le tee ne reçoit aucune balise synthétique ; chaque bloc y garde exactement ses octets, concaténés dans l'ordre de drainage sans garantir l'ordre d'émission entre flux. Sans ces options, `exec` draine un tube commun et le tee garde l'ordre d'écriture.
- **Correction d'une ancienne promesse.** Avant cette préparation, le README disait que `exec` « draine un tube commun stdout/stderr » et présentait l'original comme « exact », sans excepter `--stream` ni `--raw-on-failure` : cela laissait entendre que la chronologie entre flux était conservée partout. Le produit ne la tenait pas dans ces modes (la base `f4d6a8e` archivait stdout, puis `[stderr]`, puis stderr, et son test `exec_streams` l'exigeait), de sorte que l'ancienne documentation et l'ancien produit se contredisaient. Le contrat désormais écrit, dans le README (FR et EN), les FAQ et ici, est : octets et ordre de chaque flux exacts partout ; ordre entre flux garanti seulement dans le mode par défaut, non garanti dans les modes à flux séparés. Un rapport antérieur concluait que la chronologie « n'a jamais été un contrat » : c'est vrai du comportement et des tests, pas du texte du README.
- Sous Unix, un producteur lancé sans terminal reçoit son propre groupe de processus, que l'interruption de lm-resizer atteint en entier. Dès qu'un terminal de contrôle existe (stdin terminal, ou `/dev/tty` accessible même si stdin est un tube), il reste dans le groupe de premier plan comme en 0.2.5 : `sudo`, `ssh`, `gpg` ou une invite de mot de passe lisent le terminal au lieu d'être arrêtés par SIGTTIN, et l'interruption relayée ne vise que l'enfant immédiat.

### Sécurité
- **`rewrite-shell` pouvait exécuter du shell injecté.** Il citait les arguments entre guillemets doubles sans échapper la barre oblique inverse, qu'il tenait même pour sûre : `head -n 1 '\" ;touch X;#'` donnait une ligne dont `sh -c` créait `X`. Les arguments reconstruits sont cités en apostrophes POSIX ; une commande simplement enveloppée par `exec` garde ses octets d'origine ; un lecteur POSIX strict refuse (et laisse la ligne telle quelle) citation non fermée, opérateur ou nouvelle ligne non cités, commentaire, et toute chaîne qui ne se redécoupe pas à l'identique. Le hook utilise le même lecteur et cite le chemin du programme quand le shell le réinterpréterait. Tests jugés par `sh -c` avec des programmes de substitution, dont le cas de l'audit et un corpus de chaînes piégées.
- **La clé d'API n'est plus dans la ligne de commande du proxy.** `wrap` relançait `serve --api-key <clé>` même quand la clé ne venait que de `LM_RESIZER_API_KEY` : `/proc/<pid>/cmdline` est lisible par tous les comptes locaux. Le proxy reçoit désormais la clé par son environnement ; `--api-key-file` (ou `LM_RESIZER_API_KEY_FILE`) la lit dans un fichier refusé s'il n'est pas en 0600 ; `--api-key` reste accepté mais avertit que sa valeur est visible.
- Le proxy ne suit plus aucune redirection de l'amont (`x-api-key` était renvoyé à l'hôte choisi par un 302), refuse toute requête dont `Host` n'est pas local (rebinding DNS) et refuse une écoute hors boucle locale sans `--allow-non-loopback`.
- `sanitize-provider-fixture` masque aussi `x-api-key`, `x-goog-api-key` et les noms composés (`*_api_key`, `*secret*`, `*password*`, `*token`), sans toucher aux compteurs `max_tokens`, `input_tokens`.
- Le dossier d'état est créé en 0700 et ses fichiers (archives tee, historique des commandes, base CCR) en 0600 quel que soit le umask. Les fichiers déjà présents gardent leur mode : `chmod -R go-rwx ~/lm-resizer` une fois.
- Non traité : le proxy n'authentifie pas ses clients (tout compte local qui atteint le port dépense la clé amont), l'installateur ne vérifie qu'une somme SHA-256 servie avec l'archive (pas de signature), le hook demande `permissionDecision: allow` pour Codex. Voir [`SECURITY.md`](SECURITY.md).

### Vues pytest
- Chaque échec n'est listé qu'une fois : le bloc de `FAILURES` et la ligne de synthèse `FAILED chemin::test - raison` produisaient deux entrées (6 pour 3 échecs).
- `python -m pytest`, `python3 -m pytest`, `uv run pytest` et `uv run python -m pytest` utilisent la vue pytest ; ils étaient rendus bruts.

### Distribution
- Le binaire Linux ne contient plus de chemin du constructeur : le C compilé par les dépendances (tree-sitter) inscrivait des chemins sous le dossier personnel du constructeur, que `--remap-path-prefix` n'atteint pas. Les mêmes préfixes sont passés au compilateur C, et `scripts/check-binary-paths.cjs` contrôle le binaire et l'archive dans `check-release.sh`. Windows n'est pas couvert par ce contrôle.
- Le nom de l'outil de comparaison n'apparaît plus hors de `bench/` : README, CHANGELOG et notices le désignent comme « outil de comparaison épinglé ». Son attribution complète est dans [`bench/NOTICES.md`](bench/NOTICES.md).

### `smart --ast`
- `lm-resizer smart <fichier> --ast` résume localement (tree-sitter, sans modèle ni réseau) la structure de fichiers Rust, Python, JavaScript et TypeScript : imports, types, signatures et numéro de ligne réel de chaque déclaration, corps omis. Une source invalide ou une extension inconnue donne un repli annoncé en tête (`[smart AST: repli …]`) et la sortie historique de `smart`. Sans `--ast`, la sortie de `smart` reste identique octet pour octet à celle de la 0.2.5 (fixtures `tests/fixtures/smart/`).
- Mesures sur trois fichiers réels du dépôt, en octets : 82,29 %, 93,47 % et 89,08 % de réduction ([`bench/real/smart-ast-measurements.json`](bench/real/smart-ast-measurements.json)). Les jetons du JSON `--ast` restent une estimation octets ÷ 4 (`token_count_method: "approximate"`), contrairement au reste du produit.
- Correction relevée à la relecture : les attributs Rust `#[…]` n'apparaissent plus comme documentation de la déclaration (ils étaient rendus en `// [derive(…)]`), les commentaires s'y lisent au-dessus des attributs ; `#` n'ouvre un commentaire qu'en Python, un champ privé JavaScript `#secret` n'est plus pris pour un commentaire.
- Poids : le binaire release grossit d'environ 12 % (+3,9 Mo mesurés à la relecture) ; dépendances nouvelles tree-sitter (MIT) et streaming-iterator, ajoutées à [`THIRD-PARTY-NOTICES`](THIRD-PARTY-NOTICES).

### `git log`
- La vue montrait le premier commit seulement et comptait le reste dans « [+N lines omitted] » : `git log`, `git log -n 3`, `--pretty=short|fuller` et `--decorate` (mesuré avec `--stat` sur un dépôt réel : 1 commit sur 40 et 1 fichier sur 316 visibles, soit 99 % de « réduction » en masquant le reste). Toute sortie qui commence par `commit <hash>` (quatre caractères hexadécimaux au minimum) est maintenant rendue commit par commit : en-tête, `Merge:`, auteur, date, titre, trois lignes de corps au plus (le reste est compté), pieds `Signed-off-by` et `Co-authored-by` retirés ; avec `--stat`, chaque fichier avec son nombre de lignes, sans les barres de proportion. Mesuré avec `--stat` sur quatre dépôts : 40/40 commits et 316/316 fichiers visibles, réduction de 9 à 58 % en jetons `o200k_base`. Le brut reste dans tee.
- Une vue qui perdrait un en-tête de commit, ou qui n'économiserait aucun jeton, est abandonnée au profit du brut. `--oneline`, `--format`, `--graph` et toute sortie sans en-tête `commit` restent bruts (une ligne par commit : aucune compaction n'y est sûre) ; `--oneline` tronquait en plus la ligne d'un commit à message vide. Le plafond de 50 enregistrements de la forme à sentinelle est retiré. `git log -p` (vue de patch) montrait déjà chaque commit. Limites dans [`docs/KNOWN-MISSES.md`](docs/KNOWN-MISSES.md).
- Test : [`tests/git_log_views.rs`](tests/git_log_views.rs) construit un dépôt de sept commits (corps vide, corps long avec pied, message vide, fusion, auteur accentué) et exige, pour 13 formes de `git log`, le hash court et le sujet de chaque commit dans la vue (ou le brut) et le brut relisible par `tee read`. Avant la correction, 9 formes sur 13 échouaient.

### Banc
- [`bench/real/rejouer.sh`](bench/real/rejouer.sh) rejoue les 61 captures en une étape : Python isolé avec tiktoken, oracle construit depuis l'archive épinglée, binaire du checkout, dossier de rejeu neuf, synthèse (médiane, moyenne, bruts récupérés, codes du producteur). Le README ne renvoie plus au seul script Python du banc, qui exige ses arguments.
- Le verdict du script contrôle maintenant les 61 captures et des seuils (médiane ≥ 21,32 %, moyenne ≥ 28,97 %, médiane de l'oracle, 61 bruts récupérés, 61 codes conservés ; ceux de la 0.2.5, 25,18 % et 34,68 %, supposaient des vues `git log` qui cachaient tous les commits sauf le premier) : un résultat d'une seule capture était auparavant déclaré « tenu ». Les caches de `uv`, `pip` et `tiktoken` restent sous `target/rejeu`.
- Banc rejoué avec ce script sur le code de cette préparation (corpus de 61 captures, `o200k_base`) : médiane **21,32 %** (0.2.5 : 25,18 %), moyenne **28,97 %** (0.2.5 : 34,68 %), brut récupéré 61/61, code du producteur conservé 61/61, vues strictement égales à l'oracle 38/61 (0.2.5 : 41/61). Huit captures changent. `visible-pytest` et `fresh-pytest` : la vue pytest ne liste plus deux fois le même échec, alors que l'oracle le fait. Les six captures `git log` : avant la correction ci-dessus la même préparation donnait une médiane de 30,43 % et une moyenne de 35,14 % parce que cinq captures de plusieurs commits comptaient 91 à 97 % d'économie en ne montrant que le premier commit ; elles en comptent maintenant 0 à 38 % (`ripgrep-log` 97,18 → 38,41 %, `fastapi-log` 95,51 → 21,32 %, `compare-log` 97,18 → 36,85 %, `TypeScript-log` 93,59 → 0 % et `visible-git-authors` 91,08 → 0 %, brut rendu) et `fresh-git-log` (un seul commit) diffère de l'oracle d'un jeton. Sur ces cinq captures l'oracle rend 65 à 81 jetons, un seul commit : sa moyenne (32,61 %) dépasse désormais celle de LM Resizer, sa médiane (15,81 %) reste inférieure. Les 53 autres vues sont identiques à celles de la 0.2.5.

## [0.2.5] - publication prévue le 2026-10-08

La version 0.2.4 n'a jamais été étiquetée ni publiée : son contenu (section suivante) est livré avec 0.2.5, qui est la première version publiée depuis 0.2.3.

### Vues génériques et statistiques
- `lm-resizer err|test|summary -- <commande>` garde, pour n'importe quel producteur, les diagnostics ou les bilans de tests avec le contexte voisin, conserve le code de sortie et archive la sortie complète (`tee read`). `exec` et `tool-output` résument aussi les commandes sans filtre dédié ; l'outil MCP `lm_resizer_tool_output` marque un statut non nul `isError: true` ([`docs/CLI-REFERENCE.md`](docs/CLI-REFERENCE.md)).
- `gain` compte toutes les commandes passées par `exec`/`tool-output` (jetons mesurés `o200k_base`, `--json`, `--history`, `--project`).
- Mesure rejouée sur le corpus de 61 captures : médiane **25,18 %** (outil de comparaison épinglé, voir `bench/` : 15,81 %), moyenne **34,68 %** (outil de comparaison : 32,61 %), brut et code du producteur conservés 61/61, vues strictement égales à l'outil de comparaison 41/61 après correction du découpage du banc. Les bilans de test en échec n'ajoutent plus d'en-tête redondant ; les assertions pytest, les messages TypeScript et les blocs d'échec Cargo restent entiers. Les sorties vers un tube fermé se terminent silencieusement avec le code 141 sous Unix. Détail : [`bench/native/windows-release/delivery.md`](bench/native/windows-release/delivery.md).

### Hooks et agents
- `install-hooks` et `init` sont idempotents : un contenu identique réussit sans réécriture, un contenu divergent est refusé sans `--force`. `uninstall-hooks` retire aussi les helpers générés et ne retire une configuration native que si elle est encore exactement le JSON généré. Réserve : les helpers sont supprimés sans comparer leur contenu ([`tests/hooks_idempotence.rs`](tests/hooks_idempotence.rs)).
- `rewrite-shell` partage le refus du hook PreToolUse : redirection, tube, substitution, here-doc et commande interactive restent la ligne d'origine ; `&&`, `||` et `;` sont toujours réécrits segment par segment.

### Documentation
- README FR/EN réécrits (accroche, installation en tête, comparaison sourcée avec un filtre de référence épinglé et Headroom 0.39.1, commande de rejeu), référence CLI complète ([`docs/CLI-REFERENCE.md`](docs/CLI-REFERENCE.md)) et contrôles de cohérence ([`scripts/check-cli-reference.sh`](scripts/check-cli-reference.sh), [`scripts/check-readme-parity.cjs`](scripts/check-readme-parity.cjs), [`tests/cli_doc_alignment.rs`](tests/cli_doc_alignment.rs)).
- Précisions : `cargo install` refuse si `~/.local/bin/lm-resizer` existe déjà ; message exact de `exec` pour une commande absente ; les médianes sont celles du corpus de 61 captures, pas une promesse sur des dépôts arbitraires ; `install --client all` écrit toujours la configuration Codex de l'utilisateur.

### Distribution
- Les archives de release incluent désormais `docs/CLI-REFERENCE.md`, `docs/AGENT_HOOKS.md` et `docs/WINDOWS.md`, liés depuis le README ([`scripts/package-release.sh`](scripts/package-release.sh), [`scripts/package-release.ps1`](scripts/package-release.ps1)). Les liens vers `docs/RELEASE.md` et `bench/` restent des liens du dépôt, absents de l'archive.
- La garde de release télécharge désormais l'archive source de l'outil de comparaison épinglé sur un clone neuf et refuse un SHA-256 différent ; l'absence de réseau produit une erreur explicite.

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
