# Limites des vues natives

La migration des filtres est en cours. Les mesures antérieures ne décrivent pas cette version. Voir [le banc](../bench/native/README.md) pour les écarts mesurés, les commandes restantes et la récupération du brut.

## Crochet : commandes laissées telles quelles

`exec` garde la sortie jusqu'à la fin du processus (sauf `--stream`). Le crochet (`hook`) et `rewrite-shell` laissent donc partir telle quelle, sans `exec`, toute commande qui peut lire le terminal ou ne pas se terminer, directe ou dans la ligne de `sh|bash|zsh -c` (un seul de ses segments suffit). Liste lue sur l'`argv` :
- éditeurs, visionneuses, REPL et clients interactifs (`vim`, `less`, `top`, `ssh`, `psql` sans `-c`, `python` sans script…), `docker|kubectl exec -it` ;
- réseau qui peut demander un identifiant, une phrase de passe ou une empreinte : `git push|pull|fetch|clone|ls-remote|submodule|send-email|svn|p4`, `git remote update|prune|show`, `scp`, `mosh`, `ssh-add`, `ssh-copy-id` ;
- mot de passe ou confirmation : `sudo`, `su`, `doas`, `passwd`, `gpg`, `cargo login`, `npm|pnpm|yarn|bun init|create|login|adduser|publish`, `docker login`, `terraform|tofu apply|destroy` sans `-auto-approve` ni `-input=false`, `terraform console|login`, `aws configure`, `aws sso login`, `aws ssm start-session`, `aws ecs execute-command`, `gh auth login`, `gh pr|issue|repo create` sans `--fill`, `--title` ni `--web`, `gh pr merge` sans méthode ;
- programmes et serveurs lancés : `cargo run`, `cargo r`, `go run`, `dotnet run`, `next dev|start`, `npm|pnpm|yarn|bun` avec `dev`, `start`, `serve`, `watch` ou `preview`, `make|just|task run|serve|server|dev|start|watch|up`, `mvn spring-boot:run|exec:java|exec:exec|jetty:run|quarkus:dev|liberty:dev`, `gradle run|bootRun|appRun|quarkusDev|jettyRun`, `gradle --continuous` ;
- conteneurs : `docker|podman run|create|start` avec `-i` ou `-t` (sans `-d`), `attach`, `compose run|exec|attach` sans `-T` ni `-d`, `compose up` sans `-d`, `kubectl attach|port-forward|proxy|edit`, `kubectl run|debug -it`, `kubectl get -w` ;
- suiveurs : `tail -f`, `journalctl -f`, `docker|kubectl logs -f`, `--watch`, `--follow`, `cargo watch`, `vitest` sans `run`, `cat` sans fichier.

Limites : une commande hors de cette liste qui pose une question (un script, un test qui lit l'entrée, `terraform plan` ou `init` à qui il manque une variable) reste enveloppée et son invite n'apparaît qu'à la fin ; `docker run` sans `-i`/`-t` d'un serveur au premier plan aussi. `lm-resizer exec --stream` montre la sortie en direct.

**Arrêt brutal d'`exec`** : sous Linux, l'enfant lancé par `exec` reçoit SIGKILL quand `lm-resizer` meurt, même par `kill -9` (`PR_SET_PDEATHSIG`). Ses propres enfants ne sont pas couverts : après `kill -9` d'`exec -- sh -c 'sleep 40'`, `sh` meurt et `sleep` reste (mesuré le 9 octobre). Rien de tel sous macOS ni Windows : l'enfant y survit à un arrêt brutal.

## `git log` : ce qui reste brut

**Règle, fermée et lue dans l'`argv` seulement** : une vue qui retire ou réécrit des lignes ne s'applique qu'à un producteur que l'`argv` nomme et dont le format de sortie est établi. Tout le reste sort **brut, octet pour octet** (seul l'en-tête `[FAIL] Command failed (exit code: N)` s'ajoute quand le code est non nul). Aucune règle ne reconnaît un `git log` par le contenu de la sortie.

**Seule forme réduite** : `git [-C <dossier>]… log`, lancé par `exec` (ou `lm-resizer git log`), suivi uniquement de `--decorate`, `--all`, `-n <N>`, `-<N>`, `--oneline`, `--graph`, de révisions ou de plages (mots sans `-` initial) et, après `--`, de chemins ; et seulement si `git config --get-regexp '^(format|log)\.'`, lu avec les mêmes `-C` dans le dossier de la commande, ne trouve aucune clé. La vue montre alors chaque commit (en-tête, `Merge:`, `Author:`, `Date:`, titre et trois lignes de corps au plus ; le reste est compté dans `[+N message lines omitted]`, les pieds `Signed-off-by` et `Co-authored-by` sont retirés). Elle est refusée, et le brut rendu, si la suite des hashes `commit <hash>` de la vue diffère de celle du brut, si la sortie ne commence pas par un en-tête au format par défaut (`--oneline` et `--graph` sont donc bruts), contient un NUL ou une ligne `diff --`.

**Brut** : toute autre option (`--format`, `--pretty`, `--stat`, `-p`, `--author`, `--date`, `--no-merges`, `-z`…) ; toute autre option globale (`--no-pager`, `-c`, `--git-dir`…) ; un alias ; `pipe` et `tool-output` ; tout programme devant `git` (`env`, `timeout`, `nice`, `stdbuf`, `xargs`, un shell, `awk`, `python3 -c`…) ; un script lancé par son chemin ou par `PATH` ; une commande assemblée par variables (`g=git; "$g" log`). Plus largement, **tout programme sans vue native sort brut** : le résumé générique ne s'applique plus de lui-même (il reste disponible sur demande, `lm-resizer summary -- <commande>`). Pour un tel programme, seules deux formes entièrement structurées gardent un codec où chaque ligne distincte reste visible : un document JSON entier (`lossless:json-table`, `lossless:json-compact`) et au moins vingt lignes toutes préfixées par un niveau de journal (`INFO`, `WARN`…), dont les répétitions consécutives sont comptées (`lossless:log-runs`). Les plis de chemins, le retrait des lignes `index` d'un diff et le contour de code ne s'appliquent plus à une sortie de producteur inconnu.

**Lanceurs de recettes, bruts** : `make`/`gmake` (toute cible), `npm`, `pnpm`, `yarn`, `bun` avec `run`, `test`, `start`, `stop` ou `restart`, `cargo run`, `go run`, `uv`/`poetry`/`pipenv run`, `just`, `task`. Ils exécutent une ligne choisie par l'utilisateur, `git log --format=%s` compris. Exception : un harnais nommé dans l'`argv` lui-même garde sa vue (`npm run vitest`, `npx jest`, `uv run pytest`). `npm exec <programme>`, `npx`, `pnpm exec|dlx` et `bundle exec` désignent le programme qui suit, reconnu ou brut.

**Lanceurs de tests, règle lue dans l'`argv`** : la vue d'un lanceur de tests est brute, octet pour octet (`lossless:test-output`), quand l'argv demande d'afficher la sortie des tests, ou quand le lanceur n'a pas de capture. Sans demande, la sortie qu'un test en échec fait afficher suit la vue du lanceur ; le brut est dans tee. Drapeaux reconnus, après les enveloppes `npx`, `bunx`, `npm|pnpm|yarn|bundle exec`, `uv run`, `python -m`, `pnpm|yarn|bun jest|vitest`, et derrière `sh -c` :
- `cargo test`, `cargo nextest` : `--nocapture`, `--no-capture`, `--show-output`, `--success-output…`, avant ou après `--` ;
- `pytest`, `py.test`, `python -m pytest` : `-s` (aussi groupé, `-vs`), `--capture=no|tee-sys`, `-p no:capture`, `-r` avec `P` ou `A`, `--log-cli-level`, `-o log_cli=true` (`-rs` demande le rapport des tests sautés, pas `-s`) ;
- `go test` : `-v`, `-v=true`, `-test.v…`, `-json` ;
- `jest`, `vitest` : `--silent=false`, `--no-silent`, `--disableConsoleIntercept`, `--printConsoleTrace` ;
- `dotnet test` : `-v|--verbosity normal|detailed|diagnostic`, `--logger` de verbosité `normal`, `detailed` ou `diagnostic`.

Lanceurs sans capture, toujours bruts : `rspec`, minitest (`ruby …_test.rb`, `rake test`, `rails test`, aussi derrière `bundle exec`), `mvn` avec une phase qui lance les tests (`test`, `verify`, `package`, `install`, `deploy`), `playwright test` ; `gradle` seulement avec `-i`, `--info`, `-d` ou `--debug` (il cache la sortie des tests par défaut). Les filtres intégrés `rspec`, `minitest`, `jvm-build` (pour ces phases) et `js-quality` (pour `playwright test`) ne sont donc plus atteints par `exec`. Une variable d'environnement exportée avant l'appel (`RUST_TEST_NOCAPTURE=1`) n'est pas dans l'argv : elle n'est pas lue.

**Lanceurs de tests réussis : brut** (proposition du 9 octobre 2026, en attente de décision ; commit séparé, réversible). Un lanceur de tests nommé dans l'`argv` qui se termine par le code 0 sort brut, octet pour octet (`lossless:test-success`), par `exec`, `tool-output`, `pipe` et l'outil MCP `lm_resizer_tool_output` : `cargo test|nextest`, `pytest`, `py.test`, `python -m pytest|unittest`, `tox`, `nox`, `go test`, `jest`, `vitest`, `mocha`, `ava`, `playwright test`, `dotnet test`, `mvn` avec une phase de tests, `gradle` avec une tâche `test…`, `check` ou `build`, `rspec`, minitest, `ctest`, `phpunit`, `pest`, `php artisan test`, `deno|bun|swift|mix|zig test`, `npm|pnpm|yarn test` ou `run test…`, `make|just|task test…|check`, après les mêmes enveloppes et derrière `sh -c`. Raison : un test qui réussit peut écrire n'importe quoi sur la sortie qu'il hérite (`std::io::stdout().write_all`, un processus enfant), un `git log` relayé (contre-revue de `de2ff41` : 22 sujets sur 22 perdus, code 0) ou un avertissement (`Permission denied`, une ligne CVE : audit du 9 octobre), et la vue n'en gardait que le bilan. Un code non nul garde la vue du lanceur. `pipe` sans `--exit-code` compte comme code 0. Les dix captures de lanceurs de tests du banc se terminent toutes par un code non nul : la règle ne change aucune vue du banc (médiane 4,87 %, moyenne 27,61 %, avant comme après).

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
