# Social Post Drafts

## Short Post FR

Je travaille sur `lm-resizer`, un outil Rust pour Claude Code, Codex et les
agents MCP.

Le problème est simple : les agents IA gaspillent beaucoup de contexte avec des
logs, des sorties de tests, des diffs, des résultats `rg` ou des JSON énormes.

`lm-resizer` filtre ce bruit avant qu’il n’arrive au modèle.

Exemple :

```bash
lm-resizer exec --stream -- cargo test
```

L’agent garde les erreurs importantes, les fichiers concernés, les résumés et un
lien pour récupérer la sortie complète si nécessaire.

L’objectif : faire travailler Claude Code et Codex avec moins de tokens perdus
et plus de contexte utile.

Il est complémentaire de Code Explorer : Code Explorer donne la carte du dépôt,
lm-resizer nettoie le bruit produit pendant le travail.

Repo : https://github.com/phuetz/lm-resizer
Site : https://phuetz.github.io/lm-resizer/

## Short Post EN

I am building `lm-resizer`, a Rust-native context compression layer for Claude
Code, Codex, and MCP agents.

Coding agents waste a lot of context on raw tool output: `cargo test`, `rg`,
diffs, install logs, provider JSON, repeated failures.

Instead of sending all that noise to the model:

```bash
lm-resizer exec --stream -- cargo test
```

The agent keeps the important failures, files, summaries, and a recovery link to
the full output when needed.

Code Explorer gives the agent a map of the repository. lm-resizer keeps the
agent from wasting context while it works.

Repo: https://github.com/phuetz/lm-resizer
Site: https://phuetz.github.io/lm-resizer/

Docs: `docs/CLAUDE_CODEX.md`
Contributing fixtures/filters: `CONTRIBUTING.md`

## Technical Post FR

Quand on utilise Claude Code ou Codex sur un vrai dépôt, le problème n’est pas
seulement “est-ce que le modèle est intelligent ?”.

Le problème est aussi : **qu’est-ce qu’on lui donne à lire ?**

Un agent peut consommer énormément de contexte avec :

- des logs de tests répétitifs ;
- des sorties `rg` trop longues ;
- des diffs bruyants ;
- des JSON provider énormes ;
- des erreurs utiles noyées au milieu de lignes sans valeur.

`lm-resizer` est une couche locale qui compresse et filtre ces sorties avant
qu’elles n’arrivent au LLM.

```bash
lm-resizer exec -- cargo test
lm-resizer exec -- rg -n "TODO|FIXME" .
lm-resizer discover-sessions --agent all --markdown
lm-resizer init-native-hooks --client all --project-dir .
```

Ce qui m’intéresse le plus : le duo avec Code Explorer.

- Code Explorer répond à : “où est l’information dans le dépôt ?”
- lm-resizer répond à : “comment éviter que l’agent gaspille son contexte avec
  le bruit des commandes ?”

Ensemble :

1. l’agent interroge le graphe du code ;
2. il lance les commandes nécessaires ;
3. les sorties bruyantes sont filtrées ;
4. le brut reste récupérable si besoin.

C’est une manière plus propre de faire travailler Claude Code et Codex sur des
projets réels.

Repo : https://github.com/phuetz/lm-resizer

## Technical Post EN

Claude Code and Codex often spend context on repeated command noise: passing
tests, long logs, package install output, and huge search results.

lm-resizer adds an explicit wrapper:

```bash
lm-resizer exec -- cargo test
lm-resizer exec -- rg -n "TODO|FIXME" .
lm-resizer discover-sessions --agent all --markdown
lm-resizer init-native-hooks --client all --project-dir .
lm-resizer audit-filters --path .lm-resizer/filters.toml --review
lm-resizer sanitize-provider-fixture --provider openai --input payload.json --output fixtures/provider-cache/openai-real.json
powershell -File scripts/smoke-proxy-preview.ps1
powershell -File scripts/check-publish-readiness.ps1
powershell -File scripts/generate-checksums.ps1
```

The interesting part: it is opt-in. No shell replacement, no hidden command
execution. Project instructions can be installed reversibly in `CLAUDE.md` or
`AGENTS.md`; native Codex/Claude PostToolUse hooks can record savings without
blocking the agent; and the wrapper keeps raw-output recovery files for failed
or large commands.

Useful when you want agents to see the signal, not 20,000 repeated lines.

The repo also ships issue templates for two contribution paths: sanitized real
provider payloads and reusable command filters. That keeps the public examples
grounded in real traffic without asking anyone to publish secrets or private
prompts.

## Thread Outline

1. Problem: coding agents waste context on noisy command output.
2. Solution: wrap selected commands with `lm-resizer exec -- ...`.
3. Claude/Codex setup: install MCP plus reversible project instructions.
4. Automation: optional PATH shims route known commands through `lm-resizer exec`.
5. Recovery: raw output is saved for failed or large commands.
6. Audit: `discover` estimates savings from previous session logs.
7. Learning: `learn` turns session mining into AGENTS.md / CLAUDE.md guidance.
8. Proxy smoke: release checks boot the local proxy and verify provider preview.
9. Release hygiene: WASM/npm preflight, checksums, proxy smoke, and publish-readiness checks validate the package before publish.
10. Sample hygiene: provider payloads can be sanitized before becoming fixtures.
11. Contribution path: issue templates for provider fixtures and command filters.
12. Next work: external npm publish approval and real-world project/provider samples.

## Personal Positioning Post FR

Je construis progressivement une suite d’outils autour des agents de code.

Pas pour remplacer le développeur.
Pas pour faire une démo magique.

Pour rendre les agents réellement utilisables sur des dépôts sérieux.

Aujourd’hui :

- **Code Explorer** donne une carte du code au LLM ;
- **lm-resizer** réduit le bruit et économise le contexte ;
- **Code Buddy** sert de couche d’orchestration.

Mon angle est simple : un agent de code ne doit pas seulement être “plus
intelligent”. Il doit aussi recevoir les bonnes informations, au bon format, au
bon moment.

`lm-resizer` travaille sur cette partie invisible mais essentielle : préserver le
budget contexte de Claude Code, Codex et des agents MCP.

Projet : https://github.com/phuetz/lm-resizer
