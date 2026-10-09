# Limites des vues natives

La migration des filtres est en cours. Les mesures antérieures ne décrivent pas cette version. Voir [le banc](../bench/native/README.md) pour les écarts mesurés, les commandes restantes et la récupération du brut.

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
