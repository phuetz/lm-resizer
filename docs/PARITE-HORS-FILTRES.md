# Commandes d’administration et intégrations agents

Référence d’interopérabilité : RTK v0.50.0, commit
`1d87b8e719ce0a50c223cd93ca64dd16921f9aec`. Ces fonctions sont indépendantes
des filtres de sortie. Les comptes LM Resizer restent des mesures de texte
avec `o200k_base`, pas des mesures de facturation fournisseur.

## Statistiques et historique

```sh
lm-resizer gain --history --project
lm-resizer gain --all --graph --format text
lm-resizer gain --daily --format csv
lm-resizer gain --quota --tier pro --format json
lm-resizer gain --recalls --failures
lm-resizer gain --reset --yes
```

`stats` et `gain` sont équivalents. Le JSON reste le format par défaut pour
compatibilité. Les calendriers utilisent UTC ; les semaines commencent le
lundi. Les économies négatives sont conservées. Les anciens enregistrements
sans comptage exact restent séparés des mesures exactes.

Le quota est une **illustration** : 6 millions de tokens mensuels multipliés
par le niveau `pro`, `5x` ou `20x`. Il ne décrit pas un quota contractuel actuel.
Seules les économies du mois courant sont utilisées.

`--failures` rapporte les erreurs CLI enregistrées et les retours explicites
au brut (`raw_on_failure`, garde de diagnostics). Une sortie inchangée ne
permet pas de conclure à un échec de parseur : les retours silencieux des
parseurs internes ne sont pas instrumentés par cette fonctionnalité.

Les rappels sont rattachés à la dernière exécution antérieure qui a enregistré
la référence CCR ou tee. Les anciens rappels sans référence corrélable restent
non attribués. `--reset` archive les compteurs dans le répertoire d’état sans
supprimer les originaux ; `--project --reset` est refusé pour éviter une remise
à zéro globale accidentelle.

## Découverte et apprentissage

```sh
lm-resizer discover --project my-project --since 30 --limit 15 --format json
lm-resizer discover --all --since 0
lm-resizer session --all --format json
lm-resizer learn --since 30 --min-confidence 0.8 --min-occurrences 2
lm-resizer learn --write-rules --since 0 path/to/session.jsonl
```

Sans chemin explicite, les historiques Claude et Codex sont recherchés dans
leurs emplacements connus. Le projet courant est sélectionné par défaut ;
`--project` sélectionne un fragment de chemin, `--all` retire cette restriction,
La sélection temporelle vaut 30 jours par défaut ; `--since 0` retire la limite de date. Un historique sans projet connu n’est pas
attribué au projet courant. En l’absence de timestamp, la date de modification
du fichier est utilisée et cette méthode est indiquée dans le rapport.

Les appels et résultats structurés sont joints par identifiant, y compris
lorsque les résultats arrivent dans un ordre différent. `discover` avec des
chemins explicites et sans option de sélection conserve son ancien mode
compatible avec les journaux simples. La vue `session` indique l’adoption de
LM Resizer et les occasions manquées ; les économies calculées sur ces occasions
sont prospectives.

`learn` ne propose une correction que pour deux appels consécutifs au même
programme : erreur de syntaxe CLI explicite, puis succès explicite. La confiance
est la proportion de ces corrections observées parmi les échecs de la commande.
Une erreur de test n’est pas une preuve de correction de syntaxe. Les règles
écrites dans `.claude/rules/cli-corrections.md` sont des observations et ne sont
jamais exécutées automatiquement. Un fichier différent existant est conservé.
Les sessions illisibles sont ignorées et comptées dans `unreadable`.

## Installation des agents

Le binaire doit être accessible sous le nom `lm-resizer` dans le PATH du client.

```sh
lm-resizer init --agent claude --dry-run
lm-resizer init --codex --global
lm-resizer init --gemini
lm-resizer init --agent cursor --global
lm-resizer init --opencode
lm-resizer init --agent pi
lm-resizer init --agent hermes --global
lm-resizer init --agent claude --show
lm-resizer verify --agent claude --json
lm-resizer init --agent claude --uninstall
```

| Client | Intégration | Portée |
|---|---|---|
| Claude Code | `PreToolUse`, `.claude/settings.json`, CLAUDE.md | projet ou globale |
| Codex | `PreToolUse`, `.codex/hooks.json`, AGENTS.md | projet ou globale |
| Gemini CLI | `BeforeTool`, `.gemini/settings.json`, GEMINI.md | projet ou globale |
| Cursor | `preToolUse`, `.cursor/hooks.json` | globale |
| Trae | `PreToolUse`, `.trae/hooks.json` ; aussi `.trae-cn` en global | projet ou globale |
| Copilot | `.github/hooks/lm-resizer.json` ou `.copilot/hooks/lm-resizer.json` | projet ou globale |
| Factory Droid | `PreToolUse`, `.factory/hooks.json` | projet ou globale |
| Mistral Vibe | `.vibe/hooks.toml` et prompt | globale |
| OpenCode | plugin `tool.execute.before` | projet ou globale |
| Pi / Oh My Pi | extension `tool_call` | projet ou globale |
| Hermes | plugin Python `pre_tool_call`, manifeste et activation YAML | globale |
| Windsurf | `.windsurfrules` | projet |
| Cline / Roo Code | `.clinerules` / `.roo/rules/lm-resizer.md` | projet |
| Kilo Code | `.kilocode/rules/lm-resizer.md` | projet |
| Antigravity | `.agents/rules/lm-resizer.md` | projet |
| Kimi | bloc dans AGENTS.md | projet |

`--opencode` ajoute le plugin à l’installation sélectionnée ; `--agent opencode`
installe seulement ce plugin. `--claude-md` écrit uniquement les instructions
Claude, sans ajouter de hook. `--hook-only` évite les instructions annexes.
`--auto-patch` est non interactif ; `--no-patch` présente le plan sans écrire.
`--dry-run` ne crée aucun fichier. `--show` présente le contenu existant.

Les configurations sont fusionnées ; les autres hooks et réglages sont conservés.
Un bloc ou plugin modifié manuellement est refusé. Les plugins gérés possèdent
une empreinte voisine `*.lm-resizer.sha256` pour permettre leur mise à jour sans
écraser des modifications personnelles. La désinstallation retire uniquement
les éléments gérés. Les fichiers de configuration partagés peuvent rester vides.

Les emplacements globaux respectent `CODEX_HOME`, `CLAUDE_CONFIG_DIR`,
`XDG_CONFIG_HOME` pour OpenCode et `HERMES_HOME` pour Hermes. Les tests
d’installation isolent HOME et les éventuelles variables de répertoire client.

```sh
lm-resizer hook check --agent claude 'git status'
lm-resizer hook-audit --since 7
lm-resizer trust --list
lm-resizer trust --yes
lm-resizer untrust
```

L’audit des hooks nécessite `LM_RESIZER_HOOK_AUDIT=1` et reste local. Les hooks
ne réécrivent pas les commandes composées, redirections ou substitutions shell.
Ils laissent la commande d’origine à l’hôte en cas d’entrée ou configuration
invalide. Les contraintes allow/deny/ask et les listes de commandes détectées dans les
configurations connues Claude/Codex/Gemini/Cursor/Trae/Droid/Copilot/Vibe entraînent une abstention conservatrice,
pas une tentative d’émuler toutes les politiques de permission de ces hôtes.

Codex exige un acquittement `permissionDecision: allow` pour appliquer
`updatedInput` ; ses contrôles natifs restent nécessaires. Les modes de permission
Codex inconnus sont laissés sans réécriture. Gemini et Cursor reçoivent une demande
de décision de l’utilisateur. La conformité des formats est testée sur fixtures ;
le fonctionnement dans chaque version réelle des clients reste à vérifier.

`--trust-filters` autorise explicitement les filtres locaux après leur vérification
(fixtures réussies pour chaque filtre, sans diagnostic restant). Sans cette
couverture, `trust`, `trust-filters` et `init --trust-filters` refusent la confiance.
`--no-trust-filters` conserve leur état sans nouvelle autorisation.
Cette couche ne change ni les filtres ni leur format. `verify` vérifie les
installations ; `verify-filters` conserve son rôle pour les filtres TOML.

## Configuration, récupération et confidentialité

```sh
lm-resizer config --create
lm-resizer config --json
lm-resizer config set tracking false
lm-resizer config unset tracking
lm-resizer config recall sqlite
lm-resizer recall --list
lm-resizer recall abcdef --from 10 --lines 20 --grep 'error|warning'
lm-resizer telemetry status
lm-resizer telemetry forget --yes
```

Configuration : `LM_RESIZER_CONFIG`, sinon
`$XDG_CONFIG_HOME/lm-resizer/config.toml`, sinon
`$HOME/.config/lm-resizer/config.toml`. `--create` refuse l’écrasement.
Les variables d’environnement explicites priment sur le fichier.

Réglages : `tracking`, `tee`, `store`, `recall`. Les modes `recall` sont `tee`,
`sqlite`, `disabled` : ils sélectionnent le stockage des copies supplémentaires
de sorties originales. `disabled` ne supprime pas les payloads CCR indispensables
à la récupération de la compression. `LM_RESIZER_TEE=0` désactive ces copies
supplémentaires, quel que soit leur backend. Les entrées SQLite conservent le TTL
CCR existant ; les fichiers tee suivent leur politique de conservation existante.

`recall` accepte un hash ou un préfixe unique, refuse les ambiguïtés et préserve
les fins de ligne. Sans sélection, il rend l’original complet ; `--full` explicite
ce comportement. Sans hash, il liste les entrées.

Il n’existe aucune télémétrie distante. `telemetry enable` active seulement les
compteurs locaux. `telemetry forget --yes` efface les métriques actives et les
copies archivées de ces métriques, désactive le suivi et conserve les originaux.
Une variable d’environnement explicite peut toujours réactiver le suivi.

## Exécution et mises à jour

```sh
lm-resizer -vv exec --skip-env -- command args
lm-resizer proxy command args
lm-resizer run -c 'command1 && command2'
lm-resizer rewrite 'git status && git diff'
lm-resizer update --version 0.2.4 --dry-run
lm-resizer update --version 0.2.4 --apply
```

`proxy` ne filtre pas les flux ; il mesure l’usage et conserve le code de sortie.
Les sorties binaires ne sont pas présentées comme des comptes de tokens exacts.
`run` exécute via le shell sans filtrage ni suivi. `--skip-env` transmet
`SKIP_ENV_VALIDATION=1` aux enfants ; `--` protège les arguments littéraux.

`update` sans version consulte la dernière release GitHub. Sans `--apply`, il
présente le plan. L’application réutilise les installateurs embarqués avec
vérification SHA-256, du nom d’archive et de la version du binaire. Elle ne modifie
pas automatiquement le PATH. Les archives locales sont testables via
`--release-base-url`. Le packaging Linux ARM64 est prévu en CI ; sa sélection
par l’installateur est testée localement avec une archive de fixture.

## Limites de cette validation

Les tests locaux ne certifient ni les clients agents réels, ni les binaires natifs
Windows/macOS/ARM, ni un téléchargement de release publique. L’intégration
Copilot teste les formats snake_case et CLI camelCase ; les anciens hôtes qui
n’acceptent pas une entrée modifiée restent sans réécriture. Les permissions
propres à tous les hôtes ne sont pas réimplémentées. Les filtres, leur présentation
`--ultra-compact` et leurs échecs silencieux relèvent d’un travail distinct.

## Conservation des configurations et des journaux

Vibe exige un tableau de tables `[[hooks]]`. Une table `[hooks]`, un scalaire
ou un tableau inline incompatible entraîne un refus sans écriture. Si une ancienne
installation a ajouté son bloc exact à une telle configuration, `--uninstall`
retire ce bloc et restitue les octets d’origine. Un bloc modifié ou dupliqué
reste refusé pour préserver les modifications personnelles.

Hermes utilise une édition de l’arbre syntaxique YAML afin de conserver les
commentaires et l’ordre des autres clés. Une seconde analyse vérifie que seules
les entrées attendues de `plugins.enabled` changent ; sinon, le plan est refusé.

Les journaux prennent un verrou exclusif sur le fichier pendant l’écriture
complète, y compris les reprises d’écriture courte. Tous les producteurs LM Resizer
utilisent cette voie. Cette garantie suppose un système de fichiers qui respecte
les verrous consultatifs ; elle ne constitue pas une validation de NFS.
