# Limites des vues natives

La migration des filtres est en cours. Les mesures antérieures ne décrivent pas cette version. Voir [le banc](../bench/native/README.md) pour les écarts mesurés, les commandes restantes et la récupération du brut.

## Crochet : commandes laissées telles quelles

`exec` garde la sortie jusqu'à la fin du processus (sauf `--stream`). Le crochet (`hook`) et `rewrite-shell` suivent donc une règle prudente, lue sur l'`argv`, pour la commande directe comme pour la ligne de `sh|bash|zsh -c` (un seul de ses segments suffit) :

1. **Seule une commande qu'une vue réduit vraiment est enveloppée.** Une commande dont `exec` rendrait la sortie telle quelle part directement : script choisi par l'utilisateur (`npm test`, `npm run <script>`, `make <cible>`, `just`, `task`, `cargo run`, `go run`, `uv run`…), sortie des tests demandée (`cargo test -- --nocapture`, `pytest -s`, `mvn test`, `rspec`), programme sans vue native ou à vue identité (`php -S`, `docker run`, `docker compose …`, `cat`, `tail`), sous-commande Git sans vue (`git credential`, `git lfs`, `git gui`, `git citool`, et toute autre que `log`, `diff`, `show`, `status`). L'enveloppe n'y gagnerait rien et retiendrait l'invite ou le journal jusqu'à la fin.
2. **Liste noire, même avec une vue** : toute commande qui peut lire l'entrée ou ne pas finir.
- éditeurs, visionneuses, REPL et clients interactifs (`vim`, `less`, `top`, `ssh`, `psql` sans `-c`, `python` sans script…) ;
- réseau qui peut demander un identifiant, une phrase de passe ou une empreinte : `git push|pull|fetch|clone|ls-remote|submodule|send-email|svn|p4`, `git remote update|prune|show`, `scp`, `mosh`, `ssh-add`, `ssh-copy-id` ;
- mot de passe ou confirmation : `sudo`, `su`, `doas`, `passwd`, `gpg`, `cargo login`, `npm|pnpm|yarn|bun init|create|login|adduser|publish`, `pip uninstall` sans `-y`, `docker login`, `docker … prune` sans `-f`, `aws configure`, `aws sso login`, `aws ssm start-session`, `aws ecs execute-command`, `gh auth login`, `gh pr|issue|repo create` sans `--fill`, `--title` ni `--web`, `gh pr merge` sans méthode ;
- Terraform et OpenTofu : toute commande sans `-input=false` (elle peut demander une variable manquante ou une confirmation), `console` et `login` toujours. **`terraform plan -input=false` garde sa vue ; `terraform plan` seul part direct** ;
- débogueur : `--pdb`, `--trace`, `--pdbcls` (pytest, aussi par `python -m pytest`) ;
- programmes et serveurs lancés : `cargo run`, `cargo r`, `go run`, `dotnet run`, `next dev|start`, `npm|pnpm|yarn|bun` avec `dev`, `start`, `serve`, `watch` ou `preview`, `make|just|task run|serve|server|dev|start|watch|up`, `mvn spring-boot:run|exec:java|exec:exec|jetty:run|quarkus:dev|liberty:dev`, `gradle run|bootRun|appRun|quarkusDev|jettyRun`, `gradle --continuous` ;
- conteneurs, sans exception : `docker|podman run|create|start|exec|attach`, `compose run|exec|attach` (`-T` retire le pseudo-terminal, pas l'entrée), `compose up` sans `-d`, `kubectl exec|run|debug|attach|port-forward|proxy|edit`, `kubectl get -w` ;
- suiveurs : `tail -f`, `journalctl -f`, `docker|kubectl logs -f`, `--watch`, `--follow`, `cargo watch`, `vitest` sans `run`, `cat` sans fichier.

Limites : une commande qui a une vue, hors de cette liste, et qui pose une question (un test qui lit l'entrée, un `cargo test` qui lance un serveur) reste enveloppée et son invite n'apparaît qu'à la fin. `lm-resizer exec --stream` montre la sortie en direct.

**Arrêt brutal d'`exec`** : sous Linux, l'enfant lancé par `exec` reçoit SIGKILL quand `lm-resizer` meurt, même par `kill -9` (`PR_SET_PDEATHSIG`). Ses propres enfants ne sont pas couverts : après `kill -9` d'`exec -- sh -c 'sleep 40'`, `sh` meurt et `sleep` reste (mesuré le 9 octobre). Rien de tel sous macOS ni Windows : l'enfant y survit à un arrêt brutal.

## `git log` : ce qui reste brut

**Règle, fermée et lue dans l'`argv` seulement** : une vue qui retire ou réécrit des lignes ne s'applique qu'à un producteur que l'`argv` nomme et dont le format de sortie est établi. Tout le reste sort **brut, octet pour octet** (seul l'en-tête `[FAIL] Command failed (exit code: N)` s'ajoute quand le code est non nul). Aucune règle ne reconnaît un `git log` par le contenu de la sortie.

**Seule forme réduite** : `git [-C <dossier>]… log`, lancé par `exec` (ou `lm-resizer git log`), suivi uniquement de `--decorate`, `--all`, `-n <N>`, `-<N>`, `--oneline`, `--graph`, de révisions ou de plages (mots sans `-` initial) et, après `--`, de chemins ; et seulement si `git config --get-regexp '^(format|log)\.'`, lu avec les mêmes `-C` dans le dossier de la commande, ne trouve aucune clé. La vue montre alors chaque commit (en-tête, `Merge:`, `Author:`, `Date:`, titre et trois lignes de corps au plus ; le reste est compté dans `[+N message lines omitted]`, les pieds `Signed-off-by` et `Co-authored-by` sont retirés). Elle est refusée, et le brut rendu, si la suite des hashes `commit <hash>` de la vue diffère de celle du brut, si la sortie ne commence pas par un en-tête au format par défaut (`--oneline` et `--graph` sont donc bruts), contient un NUL ou une ligne `diff --`.

**Brut** : toute autre option (`--format`, `--pretty`, `--stat`, `-p`, `--author`, `--date`, `--no-merges`, `-z`…) ; toute autre option globale (`--no-pager`, `-c`, `--git-dir`…) ; un alias ; `pipe` et `tool-output` ; tout programme devant `git` (`env`, `timeout`, `nice`, `stdbuf`, `xargs`, un shell, `awk`, `python3 -c`…) ; un script lancé par son chemin ou par `PATH` ; une commande assemblée par variables (`g=git; "$g" log`). Plus largement, **tout programme sans vue native sort brut** : le résumé générique ne s'applique plus de lui-même (il reste disponible sur demande, `lm-resizer summary -- <commande>`). Pour un tel programme, seules deux formes entièrement structurées gardent un codec où chaque ligne distincte reste visible : un document JSON entier (`lossless:json-table`, `lossless:json-compact`) et au moins vingt lignes toutes préfixées par un niveau de journal (`INFO`, `WARN`…), dont les répétitions consécutives sont comptées (`lossless:log-runs`). Les plis de chemins, le retrait des lignes `index` d'un diff et le contour de code ne s'appliquent plus à une sortie de producteur inconnu.

**Lanceurs de recettes, bruts** : `make`/`gmake` (toute cible), `npm`, `pnpm`, `yarn`, `bun` avec `run`, `test`, `start`, `stop` ou `restart`, `cargo run`, `go run`, `uv`/`poetry`/`pipenv`/`pdm`/`hatch`/`rye run`, `just`, `task`, et `npm|pnpm|yarn|bun` précédé d'une option que la lecture ne connaît pas (`npm --foo test`). Ils exécutent une ligne choisie par l'utilisateur, `git log --format=%s` compris. Exception : un lanceur de tests nommé dans l'`argv` lui-même (`npm run vitest`, `npx jest`, `uv run pytest`) est reconnu avant, par la reconnaissance unique décrite plus bas.

**Lanceurs de tests, règle lue dans l'`argv`** : la vue d'un lanceur de tests est brute, octet pour octet (`lossless:test-output`), quand l'argv demande d'afficher la sortie des tests, ou quand le lanceur n'a pas de capture. Sans demande, la sortie qu'un test en échec fait afficher suit la vue du lanceur ; le brut est dans tee. Drapeaux reconnus, après les enveloppes de la reconnaissance unique (plus bas) et derrière `sh -c` :
- `cargo test`, `cargo nextest` : `--nocapture`, `--no-capture`, `--show-output`, `--success-output…`, avant ou après `--` ;
- `pytest`, `py.test`, `python -m pytest` : `-s` (aussi groupé, `-vs`), `--capture=no|tee-sys`, `-p no:capture`, `-r` avec `P` ou `A`, `--log-cli-level`, `-o log_cli=true` (`-rs` demande le rapport des tests sautés, pas `-s`) ;
- `go test` : `-v`, `-v=true`, `-test.v…`, `-json` ;
- `jest`, `vitest` : `--silent=false`, `--no-silent`, `--disableConsoleIntercept`, `--printConsoleTrace` ;
- `dotnet test` : `-v|--verbosity normal|detailed|diagnostic`, `--logger` de verbosité `normal`, `detailed` ou `diagnostic`.

Lanceurs sans capture, toujours bruts : `rspec`, minitest (`ruby …_test.rb`, `rake test`, `rails test`, aussi derrière `bundle exec`), `mvn` avec une phase qui lance les tests (`test`, `verify`, `package`, `install`, `deploy`), `playwright test` ; `gradle` seulement avec `-i`, `--info`, `-d` ou `--debug` (il cache la sortie des tests par défaut). Les filtres intégrés `rspec`, `minitest`, `jvm-build` (pour ces phases) et `js-quality` (pour `playwright test`) ne sont donc plus atteints par `exec`. Une variable d'environnement exportée avant l'appel (`RUST_TEST_NOCAPTURE=1`) n'est pas dans l'argv : elle n'est pas lue.

**Lanceurs de tests réussis : brut** (règle validée le 9 octobre 2026). Un lanceur de tests nommé dans l'`argv` qui se termine par le code 0 sort brut, octet pour octet (`lossless:test-success`), par `exec`, `tool-output`, `pipe` et l'outil MCP `lm_resizer_tool_output`. Raison : un test qui réussit peut écrire n'importe quoi sur la sortie qu'il hérite (`std::io::stdout().write_all`, un processus enfant), un `git log` relayé (contre-revue de `de2ff41` : 22 sujets sur 22 perdus, code 0) ou un avertissement (`Permission denied`, une ligne CVE : audit du 9 octobre), et la vue n'en gardait que le bilan. Un code non nul garde la vue du lanceur. `pipe` sans `--exit-code` compte comme code 0. Les dix captures de lanceurs de tests du banc se terminent toutes par un code non nul : la règle ne change aucune vue du banc.

**Une seule reconnaissance des lanceurs de tests** (`test_views::runner`, depuis le 10 octobre) : la porte ci-dessus, la règle d'affichage et les vues de tests (`native:cargo-test`, `native:pytest`, `native:go-test`, `native:js-test`, `native:dotnet-test`, `cargo-nextest`) passent toutes par elle, et aucune autre route ne reconnaît un lanceur. Toute forme qu'une vue de tests réduit en échec sort donc brute à code 0. Ce qu'elle lit :
- le nom du programme en minuscules, sans chemin ni suffixe `.exe`, `.cmd`, `.bat`, `.com`, `.ps1` (`cargo.cmd`, `pytest.EXE`, `npx.cmd`) ;
- les enveloppes `npx|bunx|pnpx`, `npm|pnpm|yarn|bun exec|dlx|x` avec leurs options (`--yes`, `--no-install`, `-p <paquet>`…), `pnpm|yarn|bun jest|vitest`, `npm|pnpm|yarn|bun run jest|vitest`, `bundle exec`, `uv|poetry|pipenv|pdm|hatch|rye run` avec leurs options, `python`, `python3`, `python3.12`, `pypy3` ou `py -3.12` avec leurs options avant `-m <module>` ;
- la sous-commande après les options globales : `cargo +nightly --locked test`, `cargo t`, `cargo nextest`, `go -C <dossier> test`, `npm --prefix app test` ;
- les lanceurs : `cargo test|nextest`, `pytest`, `py.test`, `go test`, `jest`, `vitest`, `dotnet test`, `mvn` avec une phase de tests, `gradle` avec une tâche `test…`, `check` ou `build`, `rspec`, minitest, `playwright test`, et sans vue dédiée `python -m unittest|nose2`, `tox`, `nox`, `mocha`, `ava`, `ctest`, `phpunit`, `pest`, `paratest`, `php artisan test`, `deno|bun|swift|mix|zig test` ; les scripts de tests `npm|pnpm|yarn|bun test|t|tst` ou `run test…`, `make|just|task test…|check`.

Une option d'enveloppe inconnue (`npx -c '<ligne>'`, `uv run --nouvelle-option pytest`) rend la commande illisible : elle n'est pas reconnue, aucune vue de tests ne s'applique et la sortie suit les autres routes (brute pour une enveloppe de script). Toutes les formes d'une famille ont la vue d'échec de la forme directe : `npx jest`, `npm exec jest`, `pnpm dlx jest`, `yarn jest` ont maintenant celle de `jest` (`native:js-test`), `cargo +nightly test` celle de `cargo test`, `uv run --frozen pytest` et `py -3 -m pytest` celle de `pytest`. Formes vérifiées une à une : `tests/test_runner_success.rs`.

**Lanceurs de tests en échec : ce que la vue ne garde pas.** Mesuré le 9 octobre par `tool-output --exit-code 1` sur des transcriptions d'échec auxquelles quatre lignes ont été ajoutées hors des blocs d'échec (`Permission denied: …`, `CVE-…`, `warning: …`, `error: …`) :

| Vue | Gardé | Perdu |
|---|---|---|
| `native:cargo-test` | blocs `---- nom stdout ----` sans leurs lignes vides, noms de la liste `failures:` qu'aucun bloc ne couvre, lignes `test result:` et `error: test failed…` | toute autre ligne, dont les quatre ajoutées et les `test … FAILED` |
| `native:pytest` | bilan ; par échec, l'en-tête du bloc de la section `FAILURES`, trois lignes au plus (`>`, `E`, `assert`, `error`, `.py:`) et la raison de `FAILED … - raison` | toute ligne hors de la section `FAILURES`, dont les quatre ajoutées, et le reste de chaque bloc |
| `native:js-test` (jest en texte) | ligne `Tests:`, lignes `●`, `Expected`, `Received` | le reste, dont les quatre ajoutées et `FAIL  fichier` |
| `cargo-nextest`, `vitest run`, `dotnet test`, `gradle test` (garde de diagnostic), `go test -json` (brut) | les quatre ajoutées | rien de ce qui a été ajouté |

Le brut reste dans tee (`lm-resizer tee read <id>`) ; `exec --raw-on-failure` rend tout échec brut.

**Hors de la règle, mesuré** (sortie qu'un test **en échec** fait afficher sans aucun drapeau ; transcriptions passées par `tool-output`, dépôt de 23 commits, `git log --format=%s` imprimé par le test) :

| Lanceur et situation | Vue | Sujets visibles |
|---|---|---:|
| `pytest` sans `-s`, test en échec, section `Captured stdout call` | `native:pytest`, 1 023 → 199 octets | **0/22** |
| `jest` direct, une suite en échec, `console.log` d'un test réussi | `native:js-test`, 683 → 127 octets | **0/22** |
| vrai `cargo test` sans drapeau, test en échec qui imprime puis échoue | `native:cargo-test`, 595 → 526 octets | 22/22, la ligne vide du commit sans message est retirée (`%s %H` : tout est gardé) |
| `npx jest`, `yarn jest`, `vitest run`, `npx vitest run`, `yarn vitest run`, `go test` en texte, mêmes situations | vue brute ou diagnostic gardé | 22/22 |

Ces pertes ne touchent que les formats sans hash (`%s`, `%B`) : dès qu'un hash figure dans les lignes (`%s %H`, hash indenté, hash suivi d'un NUL), la garde de contenu rend la sortie brute (mesuré sur les mêmes transcriptions pytest et jest en échec : 23/23 hashes, `native:git-identity-guard` par `exec`).

Proposition, non appliquée : garder tels quels les blocs `Captured …` de pytest et `console.*` de jest dans leurs vues (aucune capture du banc n'en contient). C'est un changement de vue, pas de règle.

**Visionneuses de journaux, limite documentée** : `docker logs`, `kubectl logs`, `journalctl` et `gh run view --log` affichent la sortie d'autres programmes et gardent leurs vues de journaux ; un `git log` qu'un conteneur ou un service a imprimé peut y perdre des lignes. Le brut se récupère toujours par `lm-resizer tee read <id>`. Restent aussi hors garantie les outils de build et de qualité (`cargo build`, `tsc`, `eslint`, `docker build`) et les filtres TOML d'un projet ou d'un utilisateur, qui choisissent leur commande par expression régulière.

Autres limites : la configuration est lue là où `lm-resizer` s'exécute ; les lignes de corps sont coupées à 100 caractères dans la vue, le titre ne l'est jamais ; la garde de contenu (`commit <hash>` ou mot de 4 à 40 chiffres hexadécimaux en début de ligne, et son sujet) reste un dernier rempart sur les voies réduites, reconnaît un hash en début de ligne (même indenté, ou suivi d'un NUL : `%H%x00%s`) ou en fin de ligne après un sujet (`%s %H`, 7 à 40 chiffres) ; la prose des messages au format par défaut n'est pas lue. Un mot comme `added` ou `face` en début de ligne peut lui faire rendre le brut (moins de compression, jamais de perte).

Conséquence mesurée sur le banc de 61 captures : les cinq captures `git log` de plusieurs commits, passées par `pipe`, passent de 91 à 97 % de « réduction » (premier commit seul, comme l'oracle de comparaison, qui le fait toujours) à 0 % ; les deux captures `npm test` (`TypeScript-test`, `visible-jest`) sont brutes. Médiane 4,87 %, moyenne 27,61 %.
