# Parité RTK — inventaire initial du 02/10/2026

Référence : RTK **v0.50.0**, commit `1d87b8e719ce0a50c223cd93ca64dd16921f9aec`, https://github.com/rtk-ai/rtk. Clone local sous `/tmp`. Binaire officiel musl installé par le script officiel avec SHA-256 vérifié.

LM Resizer de départ : `b39f1d5`. Cet inventaire décrit la surface CLI exhaustive du binaire via récursion des aides; une route existante est **partielle**, pas « faite », tant que les options et faits ne sont pas validés. Les arguments transmis aux outils natifs ne sont pas des options implémentées par RTK. Aucune reprise de code RTK à ce stade.

## Commandes, sous-commandes, options et arguments

| Commande RTK | Élément | Description RTK | État LM Resizer initial |
|---|---|---|---|
| (global) | -v, --verbose... |  | partiel — `src/main.rs:57` |
| (global) | -h, --help |  | partiel — `src/main.rs:57` |
| (global) | -V, --version |  | partiel — `src/main.rs:57` |
| ls | [ARGS]... | Arguments passed to ls (supports all native ls flags like -l, -a, -h, -R) | partiel — `src/main.rs:2771` (couverture et équivalence à prouver) |
| ls | -h, --help | Print help | partiel — `src/main.rs:2771` (couverture et équivalence à prouver) |
| ls | commande | List directory contents with token-optimized output (proxy to native ls) | partiel — `src/main.rs:2771` (couverture et équivalence à prouver) |
| tree | [ARGS]... | Arguments passed to tree (supports all native tree flags like -L, -d, -a) | partiel — `src/main.rs:2771` (couverture et équivalence à prouver) |
| tree | -h, --help | Print help | partiel — `src/main.rs:2771` (couverture et équivalence à prouver) |
| tree | commande | Directory tree with token-optimized output (proxy to native tree) | partiel — `src/main.rs:2771` (couverture et équivalence à prouver) |
| read | <FILES>... | Files to read (supports multiple, like cat) | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| read | -l, --level <LEVEL> | Filter: none (default, full content), minimal, aggressive [default: none] | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| read | -m, --max-lines <MAX_LINES> | Structural preview capped at N lines (keeps signatures and imports; not the first N lines — use --head-lines for that) | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| read | -n, --line-numbers | Show line numbers | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| read | -h, --help | Print help | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| read | commande | Read file with intelligent filtering | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| smart | <FILE> | File to analyze | partiel — `src/main.rs:94` (couverture et équivalence à prouver) |
| smart | -m, --model <MODEL> | Model: heuristic [default: heuristic] | partiel — `src/main.rs:94` (couverture et équivalence à prouver) |
| smart | -h, --help | Print help | partiel — `src/main.rs:94` (couverture et équivalence à prouver) |
| smart | commande | Generate 2-line technical summary (heuristic-based) | partiel — `src/main.rs:94` (couverture et équivalence à prouver) |
| git | -C <DIRECTORY> | Change to directory before executing (like git -C <path>, can be repeated) | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git | -c <CONFIG_OVERRIDE> | Git configuration override (like git -c key=value, can be repeated) | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git | -h, --help | Print help | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git | commande | Git commands with compact output | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git diff | [ARGS]... | Git arguments (supports all git diff flags like --stat, --cached, etc) | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git diff | -h, --help | Print help | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git diff | commande | Condensed diff output | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git log | [ARGS]... | Git arguments (supports all git log flags like --oneline, --graph, --all) | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git log | -h, --help | Print help | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git log | commande | One-line commit history | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git status | [ARGS]... | Git arguments (supports all git status flags like --porcelain, --short, -s) | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git status | -h, --help | Print help | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git status | commande | Compact status (supports all git status flags) | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git show | [ARGS]... | Git arguments (supports all git show flags) | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git show | -h, --help | Print help | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git show | commande | Compact show (commit summary + stat + compacted diff) | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git add | [ARGS]... | Files and flags to add (supports all git add flags like -A, -p, --all, etc) | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git add | -h, --help | Print help | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git add | commande | Add files → "ok" | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git commit | [ARGS]... | Git commit arguments (supports -a, -m, --amend, --allow-empty, etc) | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git commit | -h, --help | Print help | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git commit | commande | Commit → "ok \<hash\>" | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git checkout | [ARGS]... | Git checkout arguments (supports -b, branch names, refs, -- paths) | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git checkout | -h, --help | Print help | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git checkout | commande | Checkout branch or restore paths → "ok" | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git push | [ARGS]... | Git push arguments (supports -u, remote, branch, etc.) | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git push | -h, --help | Print help | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git push | commande | Push → "ok \<branch\>" | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git pull | [ARGS]... | Git pull arguments (supports --rebase, remote, branch, etc.) | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git pull | -h, --help | Print help | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git pull | commande | Pull → "ok \<stats\>" | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git branch | [ARGS]... | Git branch arguments (supports -d, -D, -m, etc.) | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git branch | -h, --help | Print help | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git branch | commande | Compact branch listing (current/local/remote) | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git fetch | [ARGS]... | Git fetch arguments | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git fetch | -h, --help | Print help | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git fetch | commande | Fetch → "ok fetched (N new refs)" | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git stash | [SUBCOMMAND] | Subcommand: list, show, pop, apply, drop, push | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git stash | [ARGS]... | Additional arguments | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git stash | -h, --help | Print help | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git stash | commande | Stash management (list, show, pop, apply, drop) | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git worktree | [ARGS]... | Git worktree arguments (add, remove, prune, or empty for list) | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git worktree | -h, --help | Print help | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| git worktree | commande | Compact worktree listing | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| gh | <SUBCOMMAND> | Subcommand: pr, issue, run, repo | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| gh | [ARGS]... | Additional arguments | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| gh | -h, --help | Print help | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| gh | commande | GitHub CLI (gh) commands with token-optimized output | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| glab | <SUBCOMMAND> | Subcommand: mr, issue, ci, pipeline, api | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| glab | [ARGS]... | Additional arguments | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| glab | -R, --repo <REPO> | Target repository (owner/repo), passed as glab -R flag | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| glab | -g, --group <GROUP> | Target group, passed as glab -g flag | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| glab | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| glab | commande | GitLab CLI (glab) commands with token-optimized output | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| aws | <SUBCOMMAND> | AWS service subcommand (e.g., sts, s3, ec2, ecs, rds, cloudformation) | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| aws | [ARGS]... | Additional arguments | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| aws | -h, --help | Print help | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| aws | commande | AWS CLI with compact output (force JSON, compress) | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| pnpm | -F, --filter <FILTER> | pnpm filter arguments (can be repeated: --filter @app1 --filter @app2) | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| pnpm | -r, --recursive | Recursive across workspace packages (pnpm -r) | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| pnpm | -w, --workspace-root | Run in the workspace root (pnpm -w) | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| pnpm | -h, --help | Print help | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| pnpm | commande | pnpm commands with ultra-compact output | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| pnpm list | [ARGS]... | Additional pnpm arguments | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| pnpm list | -d, --depth <DEPTH> | Depth level (default: 0) [default: 0] | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| pnpm list | -h, --help | Print help | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| pnpm list | commande | List installed packages (ultra-dense) | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| pnpm outdated | [ARGS]... | Additional pnpm arguments | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| pnpm outdated | -h, --help | Print help | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| pnpm outdated | commande | Show outdated packages (condensed: "pkg: old → new") | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| pnpm install | [ARGS]... | Additional pnpm arguments | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| pnpm install | -h, --help | Print help | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| pnpm install | commande | Install packages (filter progress bars) | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| pnpm typecheck | [ARGS]... | Additional typecheck arguments | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| pnpm typecheck | -h, --help | Print help | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| pnpm typecheck | commande | Typecheck (delegates to tsc filter) | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| err | [COMMAND]... | Command to run | partiel — `src/main.rs:2763` (couverture et équivalence à prouver) |
| err | -h, --help | Print help | partiel — `src/main.rs:2763` (couverture et équivalence à prouver) |
| err | commande | Run command and show only errors/warnings | partiel — `src/main.rs:2763` (couverture et équivalence à prouver) |
| test | [COMMAND]... | Test command (e.g. cargo test) | partiel — `src/main.rs:2761` (couverture et équivalence à prouver) |
| test | -h, --help | Print help | partiel — `src/main.rs:2761` (couverture et équivalence à prouver) |
| test | commande | Run tests and show only failures | partiel — `src/main.rs:2761` (couverture et équivalence à prouver) |
| json | <FILE> | JSON file | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| json | -d, --depth <DEPTH> | Max depth [default: 5] | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| json | -h, --help | Print help | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| json | commande | Show JSON (compact values by default, or keys-only with --keys-only) | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| deps | [PATH] | Project path [default: .] | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deps | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deps | commande | Summarize project dependencies | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| env | -f, --filter <FILTER> | Filter by name (e.g. PATH, AWS) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| env | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| env | commande | Show environment variables (filtered) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| find | [ARGS]... | All find arguments (supports both RTK and native find syntax) | partiel — `src/main.rs:2771` (couverture et équivalence à prouver) |
| find | -h, --help | Print help | partiel — `src/main.rs:2771` (couverture et équivalence à prouver) |
| find | commande | Find files with compact tree output (accepts native find flags like -name, -type) | partiel — `src/main.rs:2771` (couverture et équivalence à prouver) |
| diff | <FILE1> | First file or - for stdin (unified diff) | partiel — `src/main.rs:2757` (couverture et équivalence à prouver) |
| diff | [FILE2] | Second file (optional if stdin) | partiel — `src/main.rs:2757` (couverture et équivalence à prouver) |
| diff | -h, --help | Print help | partiel — `src/main.rs:2757` (couverture et équivalence à prouver) |
| diff | commande | Ultra-condensed diff (only changed lines) | partiel — `src/main.rs:2757` (couverture et équivalence à prouver) |
| log | [FILE] | Log file (omit for stdin) | partiel — `src/main.rs:2772` (couverture et équivalence à prouver) |
| log | -h, --help | Print help | partiel — `src/main.rs:2772` (couverture et équivalence à prouver) |
| log | commande | Filter and deduplicate log output | partiel — `src/main.rs:2772` (couverture et équivalence à prouver) |
| dotnet | -h, --help | Print help | partiel — `src/main.rs:5866` (couverture et équivalence à prouver) |
| dotnet | commande | .NET commands with compact output (build/test/restore/format) | partiel — `src/main.rs:5866` (couverture et équivalence à prouver) |
| dotnet build | [ARGS]... |  | partiel — `src/main.rs:5866` (couverture et équivalence à prouver) |
| dotnet build | -h, --help | Print help | partiel — `src/main.rs:5866` (couverture et équivalence à prouver) |
| dotnet build | commande | Build with compact output | partiel — `src/main.rs:5866` (couverture et équivalence à prouver) |
| dotnet test | [ARGS]... |  | partiel — `src/main.rs:5866` (couverture et équivalence à prouver) |
| dotnet test | -h, --help | Print help | partiel — `src/main.rs:5866` (couverture et équivalence à prouver) |
| dotnet test | commande | Test with compact output | partiel — `src/main.rs:5866` (couverture et équivalence à prouver) |
| dotnet restore | [ARGS]... |  | partiel — `src/main.rs:5866` (couverture et équivalence à prouver) |
| dotnet restore | -h, --help | Print help | partiel — `src/main.rs:5866` (couverture et équivalence à prouver) |
| dotnet restore | commande | Restore with compact output | partiel — `src/main.rs:5866` (couverture et équivalence à prouver) |
| dotnet format | [ARGS]... |  | partiel — `src/main.rs:5866` (couverture et équivalence à prouver) |
| dotnet format | -h, --help | Print help | partiel — `src/main.rs:5866` (couverture et équivalence à prouver) |
| dotnet format | commande | Format with compact output | partiel — `src/main.rs:5866` (couverture et équivalence à prouver) |
| docker | -h, --help | Print help | partiel — `src/main.rs:3933` (couverture et équivalence à prouver) |
| docker | commande | Docker commands with compact output | partiel — `src/main.rs:3933` (couverture et équivalence à prouver) |
| docker ps | -a, --all |  | partiel — `src/main.rs:3933` (couverture et équivalence à prouver) |
| docker ps | -h, --help | Print help | partiel — `src/main.rs:3933` (couverture et équivalence à prouver) |
| docker ps | commande | List running containers | partiel — `src/main.rs:3933` (couverture et équivalence à prouver) |
| docker images | -h, --help | Print help | partiel — `src/main.rs:3933` (couverture et équivalence à prouver) |
| docker images | commande | List images | partiel — `src/main.rs:3933` (couverture et équivalence à prouver) |
| docker logs | <CONTAINER> |  | partiel — `src/main.rs:3933` (couverture et équivalence à prouver) |
| docker logs | -h, --help | Print help | partiel — `src/main.rs:3933` (couverture et équivalence à prouver) |
| docker logs | commande | Show container logs (deduplicated) | partiel — `src/main.rs:3933` (couverture et équivalence à prouver) |
| docker compose | -h, --help | Print help | partiel — `src/main.rs:3933` (couverture et équivalence à prouver) |
| docker compose | commande | Docker Compose commands with compact output | partiel — `src/main.rs:3933` (couverture et équivalence à prouver) |
| docker compose ps | -a, --all |  | partiel — `src/main.rs:3933` (couverture et équivalence à prouver) |
| docker compose ps | -h, --help | Print help | partiel — `src/main.rs:3933` (couverture et équivalence à prouver) |
| docker compose ps | commande | List compose services (compact) | partiel — `src/main.rs:3933` (couverture et équivalence à prouver) |
| docker compose logs | [SERVICE] | Optional service name | partiel — `src/main.rs:3933` (couverture et équivalence à prouver) |
| docker compose logs | -h, --help | Print help | partiel — `src/main.rs:3933` (couverture et équivalence à prouver) |
| docker compose logs | commande | Show compose logs (deduplicated) | partiel — `src/main.rs:3933` (couverture et équivalence à prouver) |
| docker compose build | [SERVICE] | Optional service name | partiel — `src/main.rs:3933` (couverture et équivalence à prouver) |
| docker compose build | -h, --help | Print help | partiel — `src/main.rs:3933` (couverture et équivalence à prouver) |
| docker compose build | commande | Build compose services (summary) | partiel — `src/main.rs:3933` (couverture et équivalence à prouver) |
| kubectl | -h, --help | Print help | partiel — `src/main.rs:3337` (couverture et équivalence à prouver) |
| kubectl | commande | Kubectl commands with compact output | partiel — `src/main.rs:3337` (couverture et équivalence à prouver) |
| kubectl get | [ARGS]... | kubectl get arguments | partiel — `src/main.rs:3337` (couverture et équivalence à prouver) |
| kubectl get | -h, --help | Print help | partiel — `src/main.rs:3337` (couverture et équivalence à prouver) |
| kubectl get | commande | Get Kubernetes resources (compact for pods/services) | partiel — `src/main.rs:3337` (couverture et équivalence à prouver) |
| kubectl pods | -n, --namespace <NAMESPACE> |  | partiel — `src/main.rs:3337` (couverture et équivalence à prouver) |
| kubectl pods | -A, --all | All namespaces | partiel — `src/main.rs:3337` (couverture et équivalence à prouver) |
| kubectl pods | -h, --help | Print help | partiel — `src/main.rs:3337` (couverture et équivalence à prouver) |
| kubectl pods | commande | List pods | partiel — `src/main.rs:3337` (couverture et équivalence à prouver) |
| kubectl services | -n, --namespace <NAMESPACE> |  | partiel — `src/main.rs:3337` (couverture et équivalence à prouver) |
| kubectl services | -A, --all | All namespaces | partiel — `src/main.rs:3337` (couverture et équivalence à prouver) |
| kubectl services | -h, --help | Print help | partiel — `src/main.rs:3337` (couverture et équivalence à prouver) |
| kubectl services | commande | List services | partiel — `src/main.rs:3337` (couverture et équivalence à prouver) |
| kubectl logs | <POD> |  | partiel — `src/main.rs:3337` (couverture et équivalence à prouver) |
| kubectl logs | -c, --container <CONTAINER> |  | partiel — `src/main.rs:3337` (couverture et équivalence à prouver) |
| kubectl logs | -h, --help | Print help | partiel — `src/main.rs:3337` (couverture et équivalence à prouver) |
| kubectl logs | commande | Show pod logs (deduplicated) | partiel — `src/main.rs:3337` (couverture et équivalence à prouver) |
| oc | -h, --help | Print help | partiel — `src/main.rs:6573` (couverture et équivalence à prouver) |
| oc | commande | OpenShift CLI (oc) commands with compact output | partiel — `src/main.rs:6573` (couverture et équivalence à prouver) |
| oc get | [ARGS]... | oc get arguments | partiel — `src/main.rs:6573` (couverture et équivalence à prouver) |
| oc get | -h, --help | Print help | partiel — `src/main.rs:6573` (couverture et équivalence à prouver) |
| oc get | commande | Get OpenShift resources (compact for pods/services) | partiel — `src/main.rs:6573` (couverture et équivalence à prouver) |
| oc pods | -n, --namespace <NAMESPACE> |  | partiel — `src/main.rs:6573` (couverture et équivalence à prouver) |
| oc pods | -A, --all | All namespaces | partiel — `src/main.rs:6573` (couverture et équivalence à prouver) |
| oc pods | -h, --help | Print help | partiel — `src/main.rs:6573` (couverture et équivalence à prouver) |
| oc pods | commande | List pods | partiel — `src/main.rs:6573` (couverture et équivalence à prouver) |
| oc services | -n, --namespace <NAMESPACE> |  | partiel — `src/main.rs:6573` (couverture et équivalence à prouver) |
| oc services | -A, --all | All namespaces | partiel — `src/main.rs:6573` (couverture et équivalence à prouver) |
| oc services | -h, --help | Print help | partiel — `src/main.rs:6573` (couverture et équivalence à prouver) |
| oc services | commande | List services | partiel — `src/main.rs:6573` (couverture et équivalence à prouver) |
| oc logs | <POD> |  | partiel — `src/main.rs:6573` (couverture et équivalence à prouver) |
| oc logs | -c, --container <CONTAINER> |  | partiel — `src/main.rs:6573` (couverture et équivalence à prouver) |
| oc logs | -h, --help | Print help | partiel — `src/main.rs:6573` (couverture et équivalence à prouver) |
| oc logs | commande | Show pod logs (deduplicated) | partiel — `src/main.rs:6573` (couverture et équivalence à prouver) |
| summary | [COMMAND]... | Command to run and summarize | partiel — `src/main.rs:24` (couverture et équivalence à prouver) |
| summary | -h, --help | Print help | partiel — `src/main.rs:24` (couverture et équivalence à prouver) |
| summary | commande | Run command and show heuristic summary | partiel — `src/main.rs:24` (couverture et équivalence à prouver) |
| grep | [EXTRA_ARGS]... | Pattern, path, and any grep/rg flags (e.g. -v, -i, -A 3, --glob, --version) | partiel — `src/main.rs:2770` (couverture et équivalence à prouver) |
| grep | -h, --help | Print help | partiel — `src/main.rs:2770` (couverture et équivalence à prouver) |
| grep | commande | Compact grep - strips whitespace, truncates, groups by file | partiel — `src/main.rs:2770` (couverture et équivalence à prouver) |
| rg | [EXTRA_ARGS]... | Pattern, path, and any rg flags (e.g. -v, -i, -t rust, --glob) | partiel — `src/main.rs:2770` (couverture et équivalence à prouver) |
| rg | -h, --help | Print help | partiel — `src/main.rs:2770` (couverture et équivalence à prouver) |
| rg | commande | Compact ripgrep - runs rg natively, same output filter as grep | partiel — `src/main.rs:2770` (couverture et équivalence à prouver) |
| ast-grep | [EXTRA_ARGS]... | ast-grep subcommand, pattern, path, and any flags (e.g. run -p '$$$', --json) | partiel — `src/rtk_filters.rs:34` (couverture et équivalence à prouver) |
| ast-grep | -h, --help | Print help | partiel — `src/rtk_filters.rs:34` (couverture et équivalence à prouver) |
| ast-grep | commande | Compact ast-grep - runs ast-grep natively, groups matches by file | partiel — `src/rtk_filters.rs:34` (couverture et équivalence à prouver) |
| init | -g, --global |  | partiel — `src/main.rs:436` (couverture et équivalence à prouver) |
| init | -h, --help |  | partiel — `src/main.rs:436` (couverture et équivalence à prouver) |
| init | commande | Initialize rtk instructions for assistant CLI usage | partiel — `src/main.rs:436` (couverture et équivalence à prouver) |
| wget | <URL> | URL to download | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| wget | [ARGS]... | Additional wget arguments | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| wget | -O, --output-document <OUTPUT> | Output file (-O - for stdout) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| wget | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| wget | commande | Download with compact output (strips progress bars) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| wc | [ARGS]... | Arguments passed to wc (files, flags like -l, -w, -c) | partiel — `src/rtk_filters.rs:65` (couverture et équivalence à prouver) |
| wc | -h, --help | Print help | partiel — `src/rtk_filters.rs:65` (couverture et équivalence à prouver) |
| wc | commande | Word/line/byte count with compact output (strips paths and padding) | partiel — `src/rtk_filters.rs:65` (couverture et équivalence à prouver) |
| gain | -p, --project | Filter statistics to current project (current working directory) // added | partiel — `src/main.rs:225` (couverture et équivalence à prouver) |
| gain | -g, --graph | Show ASCII graph of daily savings | partiel — `src/main.rs:225` (couverture et équivalence à prouver) |
| gain | -H, --history | Show recent command history | partiel — `src/main.rs:225` (couverture et équivalence à prouver) |
| gain | -q, --quota | Show monthly quota savings estimate | partiel — `src/main.rs:225` (couverture et équivalence à prouver) |
| gain | -t, --tier <TIER> | Subscription tier for quota calculation: pro, 5x, 20x [default: 20x] | partiel — `src/main.rs:225` (couverture et équivalence à prouver) |
| gain | -d, --daily | Show detailed daily breakdown (all days) | partiel — `src/main.rs:225` (couverture et équivalence à prouver) |
| gain | -w, --weekly | Show weekly breakdown | partiel — `src/main.rs:225` (couverture et équivalence à prouver) |
| gain | -m, --monthly | Show monthly breakdown | partiel — `src/main.rs:225` (couverture et équivalence à prouver) |
| gain | -a, --all | Show all time breakdowns (daily + weekly + monthly) | partiel — `src/main.rs:225` (couverture et équivalence à prouver) |
| gain | -f, --format <FORMAT> | Output format: text, json, csv [default: text] | partiel — `src/main.rs:225` (couverture et équivalence à prouver) |
| gain | -F, --failures | Show parse failure log (commands that fell back to raw execution) | partiel — `src/main.rs:225` (couverture et équivalence à prouver) |
| gain | -h, --help | Print help | partiel — `src/main.rs:225` (couverture et équivalence à prouver) |
| gain | commande | Show token savings summary and history | partiel — `src/main.rs:225` (couverture et équivalence à prouver) |
| cc-economics | -d, --daily | Show detailed daily breakdown | partiel — `src/main.rs:44` (couverture et équivalence à prouver) |
| cc-economics | -w, --weekly | Show weekly breakdown | partiel — `src/main.rs:44` (couverture et équivalence à prouver) |
| cc-economics | -m, --monthly | Show monthly breakdown | partiel — `src/main.rs:44` (couverture et équivalence à prouver) |
| cc-economics | -a, --all | Show all time breakdowns (daily + weekly + monthly) | partiel — `src/main.rs:44` (couverture et équivalence à prouver) |
| cc-economics | -f, --format <FORMAT> | Output format: text, json, csv [default: text] | partiel — `src/main.rs:44` (couverture et équivalence à prouver) |
| cc-economics | -h, --help | Print help | partiel — `src/main.rs:44` (couverture et équivalence à prouver) |
| cc-economics | commande | Claude Code economics: spending (ccusage) vs savings (rtk) analysis | partiel — `src/main.rs:44` (couverture et équivalence à prouver) |
| config | -h, --help | Print help | partiel — `src/main.rs:502` (couverture et équivalence à prouver) |
| config | commande | Show or modify configuration | partiel — `src/main.rs:502` (couverture et équivalence à prouver) |
| config recall | [MODE] | New mode; omit to show the current one | partiel — `src/main.rs:502` (couverture et équivalence à prouver) |
| config recall | -h, --help | Print help | partiel — `src/main.rs:502` (couverture et équivalence à prouver) |
| config recall | commande | Show or set the recovery mode (sqlite \| tee \| disabled) | partiel — `src/main.rs:502` (couverture et équivalence à prouver) |
| jest | [ARGS]... | Additional jest arguments | partiel — `src/main.rs:4223` (couverture et équivalence à prouver) |
| jest | -h, --help | Print help | partiel — `src/main.rs:4223` (couverture et équivalence à prouver) |
| jest | commande | Jest commands with compact output | partiel — `src/main.rs:4223` (couverture et équivalence à prouver) |
| vitest | [ARGS]... | Additional vitest arguments | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| vitest | -h, --help | Print help | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| vitest | commande | Vitest commands with compact output | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| ctest | --preset <preset>, --preset=<preset> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --presets-file <file>, --presets-file=<file> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --list-presets | = List available test presets. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -C <cfg>, --build-config <cfg> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --progress | = Enable short progress output from tests. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -V,--verbose | = Enable verbose output from tests. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -VV,--extra-verbose | = Enable more verbose output from tests. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --debug | = Displaying more verbose internals of CTest. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --output-on-failure | = Output anything outputted by the test | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --stop-on-failure | = Stop running the tests after one has failed. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --test-output-size-passed <size> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --test-output-size-failed <size> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --test-output-truncation <mode> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -F | = Enable failover. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -j [<level>], --parallel [<level>] |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -Q,--quiet | = Make ctest quiet. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -O <file>, --output-log <file> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --output-junit <file> | = Output test results to JUnit XML file. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -N,--show-only[=format] | = Disable actual execution of tests.  The | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -L <regex>, --label-regex <regex> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -R <regex>, --tests-regex <regex> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -E <regex>, --exclude-regex <regex> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -LE <regex>, --label-exclude <regex> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -FA <regex>, --fixture-exclude-any <regex> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -FS <regex>, --fixture-exclude-setup <regex> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -FC <regex>, --fixture-exclude-cleanup <regex> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -D <dashboard>, --dashboard <dashboard> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -D <var>:<type>=<value> | = Define a variable for script mode | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -M <model>, --test-model <model> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -T <action>, --test-action <action> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --source-dir <path-to-source>= Specify the project source directory. | When | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --build-dir <path-to-build> | = Alias for --test-dir.  Provided as a more | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --group <group> | = Specify what build group on the dashboard | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -S <script>, --script <script> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -SP <script>, --script-new-process <script> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -A <file>, --add-notes <file>= Add a notes file with submission |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -I [Start,End,Stride,test#,test#\|Test file], --tests-information |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -U, --union | = Take the Union of -I and -R | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --rerun-failed | = Run only the tests that failed previously | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --tests-from-file <file> | = Run the tests listed in the given file | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --exclude-from-file <file> | = Run tests except those listed in the given | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --repeat until-fail:<n>, --repeat-until-fail <n> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --repeat until-pass:<n> | = Allow each test to run up to <n> times in | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --repeat after-timeout:<n> | = Allow each test to run up to <n> times if it | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --max-width <width> | = Set the max width for a test name to output | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --interactive-debug-mode [0\|1] |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --resource-spec-file <file> | = Set the resource spec file to use. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --no-label-summary | = Disable timing summary information for | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --no-subproject-summary | = Disable timing summary information for | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --test-dir <path-to-build> | = Specify the directory in which to look for | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --build-and-test <path-to-source> <path-to-build> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --build-target <tgt> | = Specify a specific target to build. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --build-nocmake | = Run the build without running cmake first. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --build-run-dir <dir> | = Specify directory to run programs from. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --build-two-config | = Run CMake twice | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --build-exe-dir <dir> | = Specify the directory for the executable. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --build-generator <generator-name> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --build-generator-platform <platform-name> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --build-generator-toolset <toolset-name> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --build-project <project-name> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --build-makeprogram <program-name> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --build-noclean | = Skip the make clean step. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --build-config-sample <exe-name> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --build-options [<options>...] |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --test-command <command> | = The test to run with the --build-and-test | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --test-timeout <timeout> | = The time limit in seconds, internal use | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --test-load <level> | = CPU load threshold for starting new parallel | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --tomorrow-tag | = Nightly or experimental starts with next day | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --overwrite <option-name> | = Overwrite CTest configuration option. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --extra-submit <file>[;<file>] |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --http-header <header> | = Append HTTP header when submitting | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --schedule-random | = Use a random order for scheduling tests | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --schedule-random-seed <seed>= Override seed for random order of tests |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --submit-index <index> | = Submit individual dashboard tests with | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --timeout <seconds> | = Set the default test timeout. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --stop-time <time> | = Set a time at which all tests should stop | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --http1.0 | = Submit using HTTP 1.0. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --no-compress-output | = Do not compress test output when submitting. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --print-labels | = Print all available test labels. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --no-tests=<[error\|ignore]> | = Regard no tests found either as 'error' or | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --collect-instrumentation <build> |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -- | = Forward extra arguments to test executables. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | -h,-H,--help,-help,-usage,/? = Print usage information and exit. |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --version[=json-v1],-version[=json-v1],/V[=json-v1],/version[=json-v1] [<file>] |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help <keyword> [<file>] | = Print help for one keyword and exit. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help-full [<file>] | = Print all help manuals and exit. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help-manual <man> [<file>] = Print one help manual and exit. |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help-manual-list [<file>] | = List help manuals available and exit. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help-command <cmd> [<file>]= Print help for one command and exit. |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help-command-list [<file>] = List commands with help available and exit. |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help-commands [<file>] | = Print cmake-commands manual and exit. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help-diagnostic <diag> [<file>] |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help-diagnostic-list [<file>] |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help-diagnostics [<file>] | = Print cmake-diagnostics manual and exit. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help-module <mod> [<file>] = Print help for one module and exit. |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help-module-list [<file>] | = List modules with help available and exit. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help-modules [<file>] | = Print cmake-modules manual and exit. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help-policy <cmp> [<file>] = Print help for one policy and exit. |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help-policy-list [<file>] | = List policies with help available and exit. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help-policies [<file>] | = Print cmake-policies manual and exit. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help-property <prop> [<file>] |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help-property-list [<file>]= List properties with help available and |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help-properties [<file>] | = Print cmake-properties manual and exit. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help-variable var [<file>] = Print help for one variable and exit. |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help-variable-list [<file>]= List variables with help available and exit. |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | --help-variables [<file>] | = Print cmake-variables manual and exit. | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ctest | commande | Usage | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| prisma | -h, --help | Print help | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| prisma | commande | Prisma commands with compact output (no ASCII art) | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| prisma generate | [ARGS]... | Additional prisma arguments | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| prisma generate | -h, --help | Print help | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| prisma generate | commande | Generate Prisma Client (strip ASCII art) | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| prisma migrate | -h, --help | Print help | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| prisma migrate | commande | Manage migrations | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| prisma migrate dev | [ARGS]... | Additional arguments | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| prisma migrate dev | -n, --name <NAME> | Migration name | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| prisma migrate dev | -h, --help | Print help | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| prisma migrate dev | commande | Create and apply migration | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| prisma migrate status | [ARGS]... | Additional arguments | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| prisma migrate status | -h, --help | Print help | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| prisma migrate status | commande | Check migration status | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| prisma migrate deploy | [ARGS]... | Additional arguments | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| prisma migrate deploy | -h, --help | Print help | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| prisma migrate deploy | commande | Deploy migrations to production | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| prisma db-push | [ARGS]... | Additional prisma arguments | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| prisma db-push | -h, --help | Print help | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| prisma db-push | commande | Push schema to database | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| tsc | [ARGS]... | TypeScript compiler arguments | partiel — `src/main.rs:2765` (couverture et équivalence à prouver) |
| tsc | -h, --help | Print help | partiel — `src/main.rs:2765` (couverture et équivalence à prouver) |
| tsc | commande | TypeScript compiler with grouped error output | partiel — `src/main.rs:2765` (couverture et équivalence à prouver) |
| next | [ARGS]... | Next.js build arguments | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| next | -h, --help | Print help | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| next | commande | Next.js build with compact output | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| lint | [ARGS]... | Linter arguments | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| lint | -h, --help | Print help | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| lint | commande | ESLint with grouped rule violations | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| prettier | [ARGS]... | Prettier arguments (e.g., --check, --write) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| prettier | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| prettier | commande | Prettier format checker with compact output | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| format | [ARGS]... | Formatter arguments (auto-detects formatter from project files) | partiel — `src/main.rs:6758` (couverture et équivalence à prouver) |
| format | -h, --help | Print help | partiel — `src/main.rs:6758` (couverture et équivalence à prouver) |
| format | commande | Universal format checker (prettier, black, ruff format) | partiel — `src/main.rs:6758` (couverture et équivalence à prouver) |
| playwright | [ARGS]... | Playwright arguments | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| playwright | -h, --help | Print help | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| playwright | commande | Playwright E2E tests with compact output | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| cargo | -h, --help | Print help | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| cargo | commande | Cargo commands with compact output | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| cargo build | [ARGS]... | Additional cargo build arguments | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| cargo build | -h, --help | Print help | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| cargo build | commande | Build with compact output (strip Compiling lines, keep errors) | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| cargo test | [ARGS]... | Additional cargo test arguments | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| cargo test | -h, --help | Print help | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| cargo test | commande | Test with failures-only output | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| cargo clippy | [ARGS]... | Additional cargo clippy arguments | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| cargo clippy | -h, --help | Print help | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| cargo clippy | commande | Clippy with warnings grouped by lint rule | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| cargo check | [ARGS]... | Additional cargo check arguments | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| cargo check | -h, --help | Print help | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| cargo check | commande | Check with compact output (strip Checking lines, keep errors) | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| cargo install | [ARGS]... | Additional cargo install arguments | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| cargo install | -h, --help | Print help | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| cargo install | commande | Install with compact output (strip dep compilation, keep installed/errors) | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| cargo nextest | [ARGS]... | Additional cargo nextest arguments (e.g., run, list, --lib) | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| cargo nextest | -h, --help | Print help | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| cargo nextest | commande | Nextest with failures-only output | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| npm | [ARGS]... | npm run arguments (script name + options) | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| npm | -h, --help | Print help | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| npm | commande | npm run with filtered output (strip boilerplate) | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| npx | [ARGS]... | npx arguments (command + options) | partiel — `src/main.rs:4226` (couverture et équivalence à prouver) |
| npx | -h, --help | Print help | partiel — `src/main.rs:4226` (couverture et équivalence à prouver) |
| npx | commande | npx with intelligent routing (tsc, eslint, prisma -> specialized filters) | partiel — `src/main.rs:4226` (couverture et équivalence à prouver) |
| bun | -h, --help | Print help | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun | commande | Bun runtime commands with compact output | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun install | [ARGS]... |  | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun install | -h, --help | Print help | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun install | commande | Install packages (filter progress bars) | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun run | [ARGS]... |  | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun run | -h, --help | Print help | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun run | commande | Run scripts | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun build | [ARGS]... |  | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun build | -h, --help | Print help | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun build | commande | Build project (errors only) | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun test | [ARGS]... |  | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun test | -h, --help | Print help | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun test | commande | Test with compact output (failures only) | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun add | [ARGS]... |  | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun add | -h, --help | Print help | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun add | commande | Add packages | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun remove | [ARGS]... |  | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun remove | -h, --help | Print help | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun remove | commande | Remove packages | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun pm | -h, --help | Print help | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun pm | commande | Package manager commands (pm ls, etc.) | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun pm ls | [ARGS]... |  | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun pm ls | -h, --help | Print help | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun pm ls | commande | List installed packages (compact output) | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun x | [ARGS]... |  | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun x | -h, --help | Print help | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bun x | commande | Execute a package binary (space form of bunx) | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| bunx | [ARGS]... | bunx arguments | partiel — `src/main.rs:4226` (couverture et équivalence à prouver) |
| bunx | -h, --help | Print help | partiel — `src/main.rs:4226` (couverture et équivalence à prouver) |
| bunx | commande | bunx with passthrough + auto-filter | partiel — `src/main.rs:4226` (couverture et équivalence à prouver) |
| curl | [ARGS]... | Curl arguments (URL + options) | partiel — `src/main.rs:11076` (couverture et équivalence à prouver) |
| curl | -h, --help | Print help | partiel — `src/main.rs:11076` (couverture et équivalence à prouver) |
| curl | commande | Curl with auto-JSON detection and schema output | partiel — `src/main.rs:11076` (couverture et équivalence à prouver) |
| discover | -p, --project <PROJECT> | Filter by project path (substring match) | partiel — `src/main.rs:355` (couverture et équivalence à prouver) |
| discover | -l, --limit <LIMIT> | Max commands per section [default: 15] | partiel — `src/main.rs:355` (couverture et équivalence à prouver) |
| discover | -a, --all | Scan all projects (default: current project only) | partiel — `src/main.rs:355` (couverture et équivalence à prouver) |
| discover | -s, --since <SINCE> | Limit to sessions from last N days [default: 30] | partiel — `src/main.rs:355` (couverture et équivalence à prouver) |
| discover | -f, --format <FORMAT> | Output format: text, json [default: text] | partiel — `src/main.rs:355` (couverture et équivalence à prouver) |
| discover | -h, --help | Print help | partiel — `src/main.rs:355` (couverture et équivalence à prouver) |
| discover | commande | Discover missed RTK savings from Claude Code history | partiel — `src/main.rs:355` (couverture et équivalence à prouver) |
| session | -h, --help | Print help | partiel — `src/main.rs:370` (couverture et équivalence à prouver) |
| session | commande | Show RTK adoption across Claude Code sessions | partiel — `src/main.rs:370` (couverture et équivalence à prouver) |
| telemetry | -h, --help | Print help | partiel — `src/main.rs:44` (couverture et équivalence à prouver) |
| telemetry | commande | Manage telemetry consent and data (RGPD/GDPR) | partiel — `src/main.rs:44` (couverture et équivalence à prouver) |
| telemetry status | -h, --help | Print help | partiel — `src/main.rs:44` (couverture et équivalence à prouver) |
| telemetry status | commande | Usage: rtk telemetry status [OPTIONS] | partiel — `src/main.rs:44` (couverture et équivalence à prouver) |
| telemetry enable | -h, --help | Print help | partiel — `src/main.rs:44` (couverture et équivalence à prouver) |
| telemetry enable | commande | Usage: rtk telemetry enable [OPTIONS] | partiel — `src/main.rs:44` (couverture et équivalence à prouver) |
| telemetry forget | -h, --help | Print help | partiel — `src/main.rs:44` (couverture et équivalence à prouver) |
| telemetry forget | commande | Usage: rtk telemetry forget [OPTIONS] | partiel — `src/main.rs:44` (couverture et équivalence à prouver) |
| learn | -p, --project <PROJECT> |  | partiel — `src/main.rs:397` (couverture et équivalence à prouver) |
| learn | -a, --all |  | partiel — `src/main.rs:397` (couverture et équivalence à prouver) |
| learn | -s, --since <SINCE> |  | partiel — `src/main.rs:397` (couverture et équivalence à prouver) |
| learn | -f, --format <FORMAT> |  | partiel — `src/main.rs:397` (couverture et équivalence à prouver) |
| learn | -w, --write-rules |  | partiel — `src/main.rs:397` (couverture et équivalence à prouver) |
| learn | -h, --help |  | partiel — `src/main.rs:397` (couverture et équivalence à prouver) |
| learn | commande | Learn CLI corrections from Claude Code error history | partiel — `src/main.rs:397` (couverture et équivalence à prouver) |
| run | [ARGS]... | Positional command arguments (alternative to -c) | partiel — `src/main.rs:130` (couverture et équivalence à prouver) |
| run | -c, --command <COMMAND> | Command string to execute (use -c for shell-like invocation) | partiel — `src/main.rs:130` (couverture et équivalence à prouver) |
| run | -h, --help | Print help | partiel — `src/main.rs:130` (couverture et équivalence à prouver) |
| run | commande | Execute a shell command via sh -c (raw, no filtering or tracking) | partiel — `src/main.rs:130` (couverture et équivalence à prouver) |
| proxy | [ARGS]... | Command and arguments to execute | partiel — `src/main.rs:130` (couverture et équivalence à prouver) |
| proxy | -h, --help | Print help | partiel — `src/main.rs:130` (couverture et équivalence à prouver) |
| proxy | commande | Execute command without filtering but track usage | partiel — `src/main.rs:130` (couverture et équivalence à prouver) |
| recall | [HASH] | Hash from a recovery hint (a unique prefix is enough) | partiel — `src/main.rs:192` (couverture et équivalence à prouver) |
| recall | -h, --help | Print help | partiel — `src/main.rs:192` (couverture et équivalence à prouver) |
| recall | commande | Recall output a filter elided, by content hash | partiel — `src/main.rs:192` (couverture et équivalence à prouver) |
| pipe | -f, --filter <FILTER> | Filter name (cargo-test, pytest, phpunit, phpstan, pint, grep, find, git-log, etc.) | partiel — `src/main.rs:152` (couverture et équivalence à prouver) |
| pipe | -h, --help | Print help | partiel — `src/main.rs:152` (couverture et équivalence à prouver) |
| pipe | commande | Read stdin, apply filter, print filtered output (Unix pipe mode) | partiel — `src/main.rs:152` (couverture et équivalence à prouver) |
| trust | -y, --yes | Trust without prompting (for non-interactive use) | partiel — `src/main.rs:277` (couverture et équivalence à prouver) |
| trust | -h, --help | Print help | partiel — `src/main.rs:277` (couverture et équivalence à prouver) |
| trust | commande | Trust project-local TOML filters in current directory | partiel — `src/main.rs:277` (couverture et équivalence à prouver) |
| untrust | -h, --help | Print help | partiel — `src/main.rs:292` (couverture et équivalence à prouver) |
| untrust | commande | Revoke trust for project-local TOML filters | partiel — `src/main.rs:292` (couverture et équivalence à prouver) |
| verify | -h, --help | Print help | partiel — `src/main.rs:313` (couverture et équivalence à prouver) |
| verify | commande | Verify hook integrity and run TOML filter inline tests | partiel — `src/main.rs:313` (couverture et équivalence à prouver) |
| ruff | [ARGS]... | Ruff arguments (e.g., check, format --check) | partiel — `src/main.rs:3313` (couverture et équivalence à prouver) |
| ruff | -h, --help | Print help | partiel — `src/main.rs:3313` (couverture et équivalence à prouver) |
| ruff | commande | Ruff linter/formatter with compact output | partiel — `src/main.rs:3313` (couverture et équivalence à prouver) |
| sqlfluff | [ARGS]... | SQLFluff arguments (e.g., lint models/, fix models/staging/) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| sqlfluff | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| sqlfluff | commande | SQLFluff SQL linter with compact output | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| pytest | [ARGS]... | Pytest arguments | partiel — `src/main.rs:2766` (couverture et équivalence à prouver) |
| pytest | -h, --help | Print help | partiel — `src/main.rs:2766` (couverture et équivalence à prouver) |
| pytest | commande | Pytest test runner with compact output | partiel — `src/main.rs:2766` (couverture et équivalence à prouver) |
| mypy | [ARGS]... | Mypy arguments | partiel — `src/main.rs:3313` (couverture et équivalence à prouver) |
| mypy | -h, --help | Print help | partiel — `src/main.rs:3313` (couverture et équivalence à prouver) |
| mypy | commande | Mypy type checker with grouped error output | partiel — `src/main.rs:3313` (couverture et équivalence à prouver) |
| php | [ARGS]... | PHP arguments (e.g., artisan about, -l app/Http/Controller.php) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| php | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| php | commande | PHP command runner with compact output for artisan and syntax checks | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| phpunit | [ARGS]... | PHPUnit arguments | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| phpunit | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| phpunit | commande | PHPUnit test runner with compact output | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| phpstan | [ARGS]... | PHPStan arguments (e.g., analyse src/) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| phpstan | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| phpstan | commande | PHPStan analyzer with compact output | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| pest | [ARGS]... | Pest arguments | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| pest | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| pest | commande | Pest test runner with compact output | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| paratest | [ARGS]... | ParaTest arguments | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| paratest | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| paratest | commande | ParaTest parallel test runner with compact output | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ecs | [ARGS]... | ECS arguments (e.g., check src/, --fix) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ecs | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| ecs | commande | EasyCodingStandard (ECS) code style fixer with compact output | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| pint | [ARGS]... | Pint arguments (e.g., --test, app/) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| pint | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| pint | commande | Laravel Pint (PHP-CS-Fixer) code style fixer with compact output | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| phpt | [ARGS]... | Arguments forwarded to `php run-tests.php` (e.g., Zend/tests/, -q) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| phpt | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| phpt | commande | PHP run-tests.php (.phpt) with compact summary and failure diffs | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| rake | [ARGS]... | Rake arguments (e.g., test, test TEST=path/to/test.rb) | partiel — `src/main.rs:6934` (couverture et équivalence à prouver) |
| rake | -h, --help | Print help | partiel — `src/main.rs:6934` (couverture et équivalence à prouver) |
| rake | commande | Rake/Rails test with compact Minitest output (Ruby) | partiel — `src/main.rs:6934` (couverture et équivalence à prouver) |
| rubocop | [ARGS]... | RuboCop arguments (e.g., --auto-correct, -A) | partiel — `src/rtk_filters.rs:25` (couverture et équivalence à prouver) |
| rubocop | -h, --help | Print help | partiel — `src/rtk_filters.rs:25` (couverture et équivalence à prouver) |
| rubocop | commande | RuboCop linter with compact output (Ruby) | partiel — `src/rtk_filters.rs:25` (couverture et équivalence à prouver) |
| rspec | [ARGS]... | RSpec arguments (e.g., spec/models, --tag focus) | partiel — `src/main.rs:6912` (couverture et équivalence à prouver) |
| rspec | -h, --help | Print help | partiel — `src/main.rs:6912` (couverture et équivalence à prouver) |
| rspec | commande | RSpec test runner with compact output (Rails/Ruby) | partiel — `src/main.rs:6912` (couverture et équivalence à prouver) |
| pip | [ARGS]... | Pip arguments (e.g., list, outdated, install) | partiel — `src/main.rs:5869` (couverture et équivalence à prouver) |
| pip | -h, --help | Print help | partiel — `src/main.rs:5869` (couverture et équivalence à prouver) |
| pip | commande | Pip package manager with compact output (auto-detects uv) | partiel — `src/main.rs:5869` (couverture et équivalence à prouver) |
| uv | [ARGS]... | uv arguments (e.g., run pytest, run --project backend python script.py) | partiel — `src/main.rs:3313` (couverture et équivalence à prouver) |
| uv | -h, --help | Print help | partiel — `src/main.rs:3313` (couverture et équivalence à prouver) |
| uv | commande | uv run with compact output while preserving uv-managed environment semantics | partiel — `src/main.rs:3313` (couverture et équivalence à prouver) |
| deno | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno | commande | Deno runtime commands with compact output | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno run | [ARGS]... |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno run | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno run | commande | Run a script | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno check | [ARGS]... |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno check | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno check | commande | Type-check without running | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno lint | [ARGS]... |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno lint | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno lint | commande | Lint source files | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno test | [ARGS]... |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno test | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno test | commande | Run tests (failures only) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno task | [ARGS]... |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno task | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno task | commande | Run a task from deno.json | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno compile | [ARGS]... |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno compile | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno compile | commande | Compile to standalone executable (errors only) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno install | [ARGS]... |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno install | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| deno install | commande | Install dependencies | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| go | -h, --help | Print help | partiel — `src/main.rs:3263` (couverture et équivalence à prouver) |
| go | commande | Go commands with compact output | partiel — `src/main.rs:3263` (couverture et équivalence à prouver) |
| go test | [ARGS]... | Additional go test arguments | partiel — `src/main.rs:3263` (couverture et équivalence à prouver) |
| go test | -h, --help | Print help | partiel — `src/main.rs:3263` (couverture et équivalence à prouver) |
| go test | commande | Run tests with compact output (90% token reduction via JSON streaming) | partiel — `src/main.rs:3263` (couverture et équivalence à prouver) |
| go build | [ARGS]... | Additional go build arguments | partiel — `src/main.rs:3263` (couverture et équivalence à prouver) |
| go build | -h, --help | Print help | partiel — `src/main.rs:3263` (couverture et équivalence à prouver) |
| go build | commande | Build with compact output (errors only) | partiel — `src/main.rs:3263` (couverture et équivalence à prouver) |
| go vet | [ARGS]... | Additional go vet arguments | partiel — `src/main.rs:3263` (couverture et équivalence à prouver) |
| go vet | -h, --help | Print help | partiel — `src/main.rs:3263` (couverture et équivalence à prouver) |
| go vet | commande | Vet with compact output | partiel — `src/main.rs:3263` (couverture et équivalence à prouver) |
| sbt | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| sbt | commande | SBT (Scala Build Tool) commands with compact output | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| sbt test | [ARGS]... | Additional sbt test arguments | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| sbt test | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| sbt test | commande | Run tests with compact output (90% token reduction via ScalaTest filtering) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| sbt compile | [ARGS]... | Additional sbt compile arguments | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| sbt compile | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| sbt compile | commande | Compile with compact output (errors only) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| sbt run | [ARGS]... | Additional sbt run arguments | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| sbt run | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| sbt run | commande | Run application with noise-stripped output | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| gt | -h, --help | Print help | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| gt | commande | Graphite (gt) stacked PR commands with compact output | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| gt log | [ARGS]... |  | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| gt log | -h, --help | Print help | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| gt log | commande | Compact stack log output | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| gt submit | [ARGS]... |  | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| gt submit | -h, --help | Print help | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| gt submit | commande | Compact submit output | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| gt sync | [ARGS]... |  | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| gt sync | -h, --help | Print help | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| gt sync | commande | Compact sync output | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| gt restack | [ARGS]... |  | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| gt restack | -h, --help | Print help | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| gt restack | commande | Compact restack output | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| gt create | [ARGS]... |  | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| gt create | -h, --help | Print help | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| gt create | commande | Compact create output | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| gt branch | [ARGS]... |  | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| gt branch | -h, --help | Print help | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| gt branch | commande | Branch info and management | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| golangci-lint | [ARGS]... | Additional golangci-lint arguments | partiel — `src/rtk_filters.rs:46` (couverture et équivalence à prouver) |
| golangci-lint | -h, --help | Print help | partiel — `src/rtk_filters.rs:46` (couverture et équivalence à prouver) |
| golangci-lint | commande | golangci-lint wrapper with compact `run` support and passthrough for other invocations | partiel — `src/rtk_filters.rs:46` (couverture et équivalence à prouver) |
| gradlew | [ARGS]... | Gradle tasks and arguments (e.g., assembleDebug, testDebugUnitTest, lint, --info) | partiel — `src/main.rs:6774` (couverture et équivalence à prouver) |
| gradlew | -h, --help | Print help | partiel — `src/main.rs:6774` (couverture et équivalence à prouver) |
| gradlew | commande | Android Gradle wrapper with compact output (build, test, lint) | partiel — `src/main.rs:6774` (couverture et équivalence à prouver) |
| mvn | [ARGS]... | Maven goals and arguments (e.g., clean install, -DskipTests test, -X) | partiel — `src/main.rs:5867` (couverture et équivalence à prouver) |
| mvn | -h, --help | Print help | partiel — `src/main.rs:5867` (couverture et équivalence à prouver) |
| mvn | commande | Apache Maven wrapper with compact output (test, integration-test, compile, package, install, verify, deploy) | partiel — `src/main.rs:5867` (couverture et équivalence à prouver) |
| mvnd | [ARGS]... | Maven goals and arguments (e.g., clean install, -DskipTests test, -X) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| mvnd | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| mvnd | commande | Maven Daemon (mvnd) with compact output — same filters as `rtk mvn` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook-audit | -s, --since <SINCE> | Show entries from last N days (0 = all time) [default: 7] | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook-audit | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook-audit | commande | Show hook rewrite audit metrics (requires RTK_HOOK_AUDIT=1) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| rewrite | [ARGS]... |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| rewrite | -h, --help |  | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| rewrite | commande | Rewrite a raw command to its RTK equivalent (single source of truth for hooks) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook | commande | Hook processors for LLM CLI tools (Gemini CLI, Copilot, etc.) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook claude | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook claude | commande | Process Claude Code PreToolUse hook (reads JSON from stdin) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook trae | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook trae | commande | Process Trae PreToolUse hook (reads JSON from stdin) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook codex | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook codex | commande | Process Codex CLI PreToolUse hook (reads JSON from stdin) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook cursor | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook cursor | commande | Process Cursor Agent hook (reads JSON from stdin) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook gemini | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook gemini | commande | Process Gemini CLI BeforeTool hook (reads JSON from stdin) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook copilot | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook copilot | commande | Process Copilot preToolUse hook (VS Code + Copilot CLI, reads JSON from stdin) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook droid | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook droid | commande | Process Factory Droid PreToolUse hook (reads JSON from stdin) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook vibe | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook vibe | commande | Process Mistral Vibe CLI pre_tool hook (reads JSON from stdin) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook check | [COMMAND]... | Command to check | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook check | -h, --help | Print help | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| hook check | commande | Check how a command would be rewritten by the hook engine (dry-run) | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |

## Filtres par outil (fonctions du code RTK épinglé)

Chaque fonction de production nommée `filter*`, `compact*`, `summarize*` ou `compress*` est listée; pas de revendication de parité sur la seule présence d’une route.

| Fonction RTK | Source RTK | Équivalent LM Resizer |
|---|---|---|
| `filter_sts_identity` | `src/cmds/cloud/aws_cmd.rs:472` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_s3_ls` | `src/cmds/cloud/aws_cmd.rs:479` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_ec2_instances` | `src/cmds/cloud/aws_cmd.rs:496` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_ecs_list_services` | `src/cmds/cloud/aws_cmd.rs:556` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_ecs_describe_services` | `src/cmds/cloud/aws_cmd.rs:576` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_rds_instances` | `src/cmds/cloud/aws_cmd.rs:603` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_cfn_list_stacks` | `src/cmds/cloud/aws_cmd.rs:632` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_cfn_describe_stacks` | `src/cmds/cloud/aws_cmd.rs:657` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_logs_events` | `src/cmds/cloud/aws_cmd.rs:710` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_cfn_events` | `src/cmds/cloud/aws_cmd.rs:762` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_lambda_list` | `src/cmds/cloud/aws_cmd.rs:825` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_lambda_get` | `src/cmds/cloud/aws_cmd.rs:854` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_iam_roles` | `src/cmds/cloud/aws_cmd.rs:945` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_iam_users` | `src/cmds/cloud/aws_cmd.rs:984` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_dynamodb_items` | `src/cmds/cloud/aws_cmd.rs:1093` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_ecs_tasks` | `src/cmds/cloud/aws_cmd.rs:1135` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_security_groups` | `src/cmds/cloud/aws_cmd.rs:1238` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_s3_objects` | `src/cmds/cloud/aws_cmd.rs:1284` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_eks_cluster` | `src/cmds/cloud/aws_cmd.rs:1311` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_sqs_messages` | `src/cmds/cloud/aws_cmd.rs:1328` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_dynamodb_get_item` | `src/cmds/cloud/aws_cmd.rs:1354` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_logs_query_results` | `src/cmds/cloud/aws_cmd.rs:1380` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_s3_transfer` | `src/cmds/cloud/aws_cmd.rs:1431` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_secrets_get` | `src/cmds/cloud/aws_cmd.rs:1502` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `compact_ports` | `src/cmds/cloud/container.rs:679` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_curl_output` | `src/cmds/cloud/curl_cmd.rs:110` | partiel — `src/main.rs:11076` (couverture et équivalence à prouver) |
| `filter_psql_output` | `src/cmds/cloud/psql_cmd.rs:48` | partiel — `src/main.rs:2676` (couverture et équivalence à prouver) |
| `filter_table` | `src/cmds/cloud/psql_cmd.rs:79` | partiel — `src/main.rs:2676` (couverture et équivalence à prouver) |
| `filter_expanded` | `src/cmds/cloud/psql_cmd.rs:129` | partiel — `src/main.rs:2676` (couverture et équivalence à prouver) |
| `compact_url` | `src/cmds/cloud/wget_cmd.rs:196` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_markdown_body` | `src/cmds/git/gh_cmd.rs:28` | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| `filter_markdown_segment` | `src/cmds/git/gh_cmd.rs:101` | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| `compact_blob_show` | `src/cmds/git/git_cmd.rs:1046` | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| `compact_diff` | `src/cmds/git/git_cmd.rs:1373` | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| `filter_log_output` | `src/cmds/git/git_cmd.rs:2124` | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| `filter_status_with_args` | `src/cmds/git/git_cmd.rs:2342` | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| `filter_checkout_failure` | `src/cmds/git/git_cmd.rs:2793` | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| `filter_branch_output` | `src/cmds/git/git_cmd.rs:3185` | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| `filter_stash_list` | `src/cmds/git/git_cmd.rs:3486` | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| `compact_stash_stat` | `src/cmds/git/git_cmd.rs:3507` | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| `compress_stat_summary` | `src/cmds/git/git_cmd.rs:3528` | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| `filter_worktree_list` | `src/cmds/git/git_cmd.rs:3715` | partiel — `src/main.rs:2756` (couverture et équivalence à prouver) |
| `filter_markdown_body` | `src/cmds/git/glab_cmd.rs:43` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_markdown_segment` | `src/cmds/git/glab_cmd.rs:106` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_ci_trace` | `src/cmds/git/glab_cmd.rs:793` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_release_view` | `src/cmds/git/glab_cmd.rs:942` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_identity` | `src/cmds/git/gt_cmd.rs:81` | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| `filter_gt_log_entries` | `src/cmds/git/gt_cmd.rs:187` | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| `filter_gt_submit` | `src/cmds/git/gt_cmd.rs:218` | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| `filter_gt_sync` | `src/cmds/git/gt_cmd.rs:277` | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| `filter_gt_restack` | `src/cmds/git/gt_cmd.rs:332` | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| `filter_gt_create` | `src/cmds/git/gt_cmd.rs:353` | partiel — `src/rtk_filters.rs:44` (couverture et équivalence à prouver) |
| `filter_go_test_json` | `src/cmds/go/go_cmd.rs:294` | partiel — `src/main.rs:3263` (couverture et équivalence à prouver) |
| `filter_go_build` | `src/cmds/go/go_cmd.rs:563` | partiel — `src/main.rs:3263` (couverture et équivalence à prouver) |
| `filter_go_build_with_exit` | `src/cmds/go/go_cmd.rs:567` | partiel — `src/main.rs:3263` (couverture et équivalence à prouver) |
| `filter_go_vet` | `src/cmds/go/go_cmd.rs:697` | partiel — `src/main.rs:3263` (couverture et équivalence à prouver) |
| `compact_package_name` | `src/cmds/go/go_cmd.rs:738` | partiel — `src/main.rs:3263` (couverture et équivalence à prouver) |
| `filter_golangci_json` | `src/cmds/go/golangci_cmd.rs:342` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `compact_path` | `src/cmds/go/golangci_cmd.rs:451` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_bun_pkg` | `src/cmds/js/bun_cmd.rs:31` | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| `filter_bun_pm_ls_json` | `src/cmds/js/bun_cmd.rs:68` | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| `filter_bun_pm_ls_tree` | `src/cmds/js/bun_cmd.rs:92` | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| `filter_bun_pm_ls` | `src/cmds/js/bun_cmd.rs:125` | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| `filter_bun_pm_ls_text` | `src/cmds/js/bun_cmd.rs:136` | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| `filter_deno_output` | `src/cmds/js/deno_cmd.rs:8` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_eslint_json` | `src/cmds/js/lint_cmd.rs:255` | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| `filter_pylint_json` | `src/cmds/js/lint_cmd.rs:354` | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| `filter_generic_lint` | `src/cmds/js/lint_cmd.rs:483` | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| `compact_path` | `src/cmds/js/lint_cmd.rs:526` | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| `filter_next_build` | `src/cmds/js/next_cmd.rs:43` | partiel — `src/main.rs:3286` (couverture et équivalence à prouver) |
| `filter_npm_output` | `src/cmds/js/npm_cmd.rs:177` | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| `filter_pnpm_install` | `src/cmds/js/pnpm_cmd.rs:522` | partiel — `src/main.rs:2767` (couverture et équivalence à prouver) |
| `filter_prettier_output` | `src/cmds/js/prettier_cmd.rs:29` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_prisma_generate` | `src/cmds/js/prisma_cmd.rs:179` | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| `filter_migrate_dev` | `src/cmds/js/prisma_cmd.rs:239` | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| `filter_migrate_status` | `src/cmds/js/prisma_cmd.rs:311` | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| `filter_migrate_deploy` | `src/cmds/js/prisma_cmd.rs:348` | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| `filter_db_push` | `src/cmds/js/prisma_cmd.rs:376` | partiel — `src/main.rs:6952` (couverture et équivalence à prouver) |
| `filter_tsc_output` | `src/cmds/js/tsc_cmd.rs:244` | partiel — `src/main.rs:2765` (couverture et équivalence à prouver) |
| `filter_build_line` | `src/cmds/jvm/gradlew_cmd.rs:179` | partiel — `src/main.rs:6774` (couverture et équivalence à prouver) |
| `filter_test` | `src/cmds/jvm/gradlew_cmd.rs:232` | partiel — `src/main.rs:6774` (couverture et équivalence à prouver) |
| `filter_connected` | `src/cmds/jvm/gradlew_cmd.rs:310` | partiel — `src/main.rs:6774` (couverture et équivalence à prouver) |
| `filter_lint` | `src/cmds/jvm/gradlew_cmd.rs:360` | partiel — `src/main.rs:6774` (couverture et équivalence à prouver) |
| `filter_dependencies` | `src/cmds/jvm/gradlew_cmd.rs:446` | partiel — `src/main.rs:6774` (couverture et équivalence à prouver) |
| `filter_surefire` | `src/cmds/jvm/mvn_cmd.rs:1303` | partiel — `src/main.rs:5867` (couverture et équivalence à prouver) |
| `filter_surefire_with_cap` | `src/cmds/jvm/mvn_cmd.rs:1307` | partiel — `src/main.rs:5867` (couverture et équivalence à prouver) |
| `filter_compile` | `src/cmds/jvm/mvn_cmd.rs:1416` | partiel — `src/main.rs:5867` (couverture et équivalence à prouver) |
| `filter_package` | `src/cmds/jvm/mvn_cmd.rs:1552` | partiel — `src/main.rs:5867` (couverture et équivalence à prouver) |
| `filter_package_with_cap` | `src/cmds/jvm/mvn_cmd.rs:1556` | partiel — `src/main.rs:5867` (couverture et équivalence à prouver) |
| `filter_quiet` | `src/cmds/jvm/mvn_cmd.rs:1707` | partiel — `src/main.rs:5867` (couverture et équivalence à prouver) |
| `filter_artisan_output` | `src/cmds/php/artisan_cmd.rs:15` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_artisan_test_output` | `src/cmds/php/artisan_cmd.rs:24` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_ecs_output` | `src/cmds/php/ecs_cmd.rs:26` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_php_lint_output` | `src/cmds/php/php_cmd.rs:52` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_php_output` | `src/cmds/php/php_cmd.rs:87` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_phpstan_json` | `src/cmds/php/phpstan_cmd.rs:138` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_phpstan_text` | `src/cmds/php/phpstan_cmd.rs:208` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `compact_php_path` | `src/cmds/php/phpstan_cmd.rs:257` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_phpt_output` | `src/cmds/php/phpt_cmd.rs:129` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_phpunit_output` | `src/cmds/php/phpunit_cmd.rs:41` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_pint_json` | `src/cmds/php/pint_cmd.rs:76` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_test_runner_output` | `src/cmds/php/test_output.rs:23` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_mypy_output` | `src/cmds/python/mypy_cmd.rs:54` | partiel — `src/main.rs:3313` (couverture et équivalence à prouver) |
| `filter_pip_list` | `src/cmds/python/pip_cmd.rs:143` | partiel — `src/main.rs:5869` (couverture et équivalence à prouver) |
| `filter_pip_outdated` | `src/cmds/python/pip_cmd.rs:192` | partiel — `src/main.rs:5869` (couverture et équivalence à prouver) |
| `filter_pytest_output` | `src/cmds/python/pytest_cmd.rs:77` | partiel — `src/main.rs:2766` (couverture et équivalence à prouver) |
| `filter_ruff_check_json` | `src/cmds/python/ruff_cmd.rs:125` | partiel — `src/main.rs:3313` (couverture et équivalence à prouver) |
| `filter_ruff_format` | `src/cmds/python/ruff_cmd.rs:263` | partiel — `src/main.rs:3313` (couverture et équivalence à prouver) |
| `compact_path` | `src/cmds/python/ruff_cmd.rs:354` | partiel — `src/main.rs:3313` (couverture et équivalence à prouver) |
| `filter_sqlfluff_lint_json` | `src/cmds/python/sqlfluff_cmd.rs:157` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `compact_path` | `src/cmds/python/sqlfluff_cmd.rs:372` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_uv_run_output` | `src/cmds/python/uv_cmd.rs:92` | partiel — `src/main.rs:3313` (couverture et équivalence à prouver) |
| `filter_successful_run` | `src/cmds/python/uv_cmd.rs:155` | partiel — `src/main.rs:3313` (couverture et équivalence à prouver) |
| `filter_minitest_output` | `src/cmds/ruby/rake_cmd.rs:105` | partiel — `src/main.rs:6934` (couverture et équivalence à prouver) |
| `filter_rspec_output` | `src/cmds/ruby/rspec_cmd.rs:162` | partiel — `src/main.rs:6912` (couverture et équivalence à prouver) |
| `filter_rspec_text` | `src/cmds/ruby/rspec_cmd.rs:275` | partiel — `src/main.rs:6912` (couverture et équivalence à prouver) |
| `compact_failure_block` | `src/cmds/ruby/rspec_cmd.rs:404` | partiel — `src/main.rs:6912` (couverture et équivalence à prouver) |
| `filter_rubocop_json` | `src/cmds/ruby/rubocop_cmd.rs:101` | partiel — `src/rtk_filters.rs:25` (couverture et équivalence à prouver) |
| `filter_rubocop_text` | `src/cmds/ruby/rubocop_cmd.rs:222` | partiel — `src/rtk_filters.rs:25` (couverture et équivalence à prouver) |
| `compact_ruby_path` | `src/cmds/ruby/rubocop_cmd.rs:299` | partiel — `src/rtk_filters.rs:25` (couverture et équivalence à prouver) |
| `filter_cargo_install` | `src/cmds/rust/cargo_cmd.rs:441` | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| `filter_cargo_nextest` | `src/cmds/rust/cargo_cmd.rs:633` | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| `filter_cargo_build` | `src/cmds/rust/cargo_cmd.rs:973` | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| `filter_cargo_build_labeled` | `src/cmds/rust/cargo_cmd.rs:977` | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| `filter_cargo_test` | `src/cmds/rust/cargo_cmd.rs:1139` | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| `filter_cargo_clippy` | `src/cmds/rust/cargo_cmd.rs:1279` | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| `filter_cargo_clippy_json` | `src/cmds/rust/cargo_cmd.rs:1448` | partiel — `src/main.rs:2682` (couverture et équivalence à prouver) |
| `filter_sbt_test` | `src/cmds/scala/sbt_cmd.rs:189` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_sbt_compile` | `src/cmds/scala/sbt_cmd.rs:406` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_sbt_run` | `src/cmds/scala/sbt_cmd.rs:479` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filters_this_invocation` | `src/cmds/system/ast_grep_cmd.rs:94` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_ast_grep` | `src/cmds/system/ast_grep_cmd.rs:155` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_ctest_output` | `src/cmds/system/ctest_cmd.rs:194` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `summarize_cargo_str` | `src/cmds/system/deps.rs:83` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `summarize_package_json_str` | `src/cmds/system/deps.rs:140` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `summarize_requirements_str` | `src/cmds/system/deps.rs:181` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `summarize_pyproject_str` | `src/cmds/system/deps.rs:210` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `summarize_gomod_str` | `src/cmds/system/deps.rs:247` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filtered_hint` | `src/cmds/system/find_cmd.rs:603` | partiel — `src/main.rs:2771` (couverture et équivalence à prouver) |
| `filter_black_output` | `src/cmds/system/format_cmd.rs:142` | partiel — `src/main.rs:6758` (couverture et équivalence à prouver) |
| `compact_path` | `src/cmds/system/format_cmd.rs:272` | partiel — `src/main.rs:6758` (couverture et équivalence à prouver) |
| `filter_json_compact` | `src/cmds/system/json_cmd.rs:115` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `compact_json` | `src/cmds/system/json_cmd.rs:120` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `filter_json_string` | `src/cmds/system/json_cmd.rs:206` | partiel — `src/main.rs:2160` (couverture et équivalence à prouver) |
| `compact_ls` | `src/cmds/system/ls.rs:284` | partiel — `src/main.rs:2771` (couverture et équivalence à prouver) |
| `compact_path` | `src/cmds/system/search.rs:1054` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `summarize_output` | `src/cmds/system/summary.rs:43` | partiel — `src/main.rs:24` (couverture et équivalence à prouver) |
| `summarize_tests` | `src/cmds/system/summary.rs:114` | partiel — `src/main.rs:24` (couverture et équivalence à prouver) |
| `summarize_build` | `src/cmds/system/summary.rs:164` | partiel — `src/main.rs:24` (couverture et équivalence à prouver) |
| `summarize_logs_quick` | `src/cmds/system/summary.rs:210` | partiel — `src/main.rs:24` (couverture et équivalence à prouver) |
| `summarize_list` | `src/cmds/system/summary.rs:233` | partiel — `src/main.rs:24` (couverture et équivalence à prouver) |
| `summarize_json` | `src/cmds/system/summary.rs:245` | partiel — `src/main.rs:24` (couverture et équivalence à prouver) |
| `summarize_generic` | `src/cmds/system/summary.rs:275` | partiel — `src/main.rs:24` (couverture et équivalence à prouver) |
| `filter_tree_output` | `src/cmds/system/tree.rs:63` | partiel — `src/main.rs:2771` (couverture et équivalence à prouver) |
| `filter_wc_output` | `src/cmds/system/wc_cmd.rs:120` | partiel — `src/rtk_filters.rs:65` (couverture et équivalence à prouver) |
| `filter_errors` | `src/core/runner.rs:587` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_parse_error` | `src/core/toml_filter.rs:426` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |
| `filter_since_days` | `src/hooks/hook_audit_cmd.rs:57` | absent — aucune route dédiée identifiée; `exec` peut exécuter le programme brut |

## Intégrations, configuration, statistiques, installation et performance

| Fonction RTK | Source RTK | LM Resizer |
|---|---|---|
| Intégration `constants` | `src/hooks/constants.rs:1` | absent ou non vérifié; aucun test de ce client |
| Intégration `decision` | `src/hooks/decision.rs:1` | absent ou non vérifié; aucun test de ce client |
| Intégration `hook_audit_cmd` | `src/hooks/hook_audit_cmd.rs:1` | absent ou non vérifié; aucun test de ce client |
| Intégration `hook_check` | `src/hooks/hook_check.rs:1` | absent ou non vérifié; aucun test de ce client |
| Intégration `hook_cmd` | `src/hooks/hook_cmd.rs:1` | absent ou non vérifié; aucun test de ce client |
| Intégration `init` | `src/hooks/init.rs:1` | partiel — `src/main.rs:436`, hooks natifs Claude/Codex et instructions réversibles |
| Intégration `integrity` | `src/hooks/integrity.rs:1` | absent ou non vérifié; aucun test de ce client |
| Intégration `mod` | `src/hooks/mod.rs:1` | partiel — `src/main.rs:436`, hooks natifs Claude/Codex et instructions réversibles |
| Intégration `permissions` | `src/hooks/permissions.rs:1` | absent ou non vérifié; aucun test de ce client |
| Intégration `rewrite_cmd` | `src/hooks/rewrite_cmd.rs:1` | absent ou non vérifié; aucun test de ce client |
| Intégration `trust` | `src/hooks/trust.rs:1` | absent ou non vérifié; aucun test de ce client |
| Intégration `verify_cmd` | `src/hooks/verify_cmd.rs:1` | absent ou non vérifié; aucun test de ce client |
| Statistiques exactes, historique | `src/analytics` | src/main.rs:225; src/token_metrics.rs:1 |
| Occasions manquées / sessions | `src/discover` | src/main.rs:355 |
| Corrections apprises | `src/learn` | src/main.rs:397 |
| Configuration / filtres TOML / confiance | `src/core` | src/main.rs:977; src/main.rs:277 |
| Installation Unix, Windows, binaires, checksum | `install.sh:1` | install.sh:1; install.ps1:1; .github/workflows |
| Performance grosses sorties | `src/cmds/system/ls.rs:1` | partiel; lane perf/grosses-sorties-2026-10-02, mesures finales attendues |
| Télémétrie consentement | `src/analytics` | absent; ne pas ajouter une collecte sans nécessité |
| Client agent / variantes init | `src/main.rs:39` | partiel; options détaillées ci-dessus |

## Priorités et suivi

1. Banc et oracle exhaustif, avant/après, stdout/stderr et codes de sortie.
2. Cause de la régression grep (attente du rapport de la lane bissection); intégration du correctif, puis compactage réversible.
3. Filtres sans perte pour chemins, fichiers, tests et diagnostics.
4. Intégration de la lane performance et mesure sur TypeScript.
5. Routes et interfaces manquantes; clients et installation.
6. Rejeu réel final et documentation honnête.

Le tableau initial n’est pas une certification fonctionnelle. Les lignes restent partielles jusqu’à un commit et un test identifiés.
