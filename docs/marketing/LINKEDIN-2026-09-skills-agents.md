# LinkedIn — skills lm-resizer + Code Explorer pour Claude Code et Codex (brouillon, à publier par Patrice)

Faits vérifiés le 10/09/2026 : dossiers `.claude/skills/<outil>` et `.codex/skills/<outil>` dans les deux dépôts ; la compétence lm-resizer
n'exécute que `lm-resizer exec --raw-on-failure -- <commande>` ; Code Explorer : `status` → `analyze` → `context` / `impact` / `query`.
Chiffres autorisés : ceux des README (398 commandes, 1,23 Mo → 372 Ko, 222 247 tokens ; ~730 000 → ~18 000 tokens, ~25 ms). Rien d'autre.

## Version profil

Un outil que l'agent ne sait pas quand utiliser ne sert à rien.

J'ai donc mis dans les dépôts de lm-resizer et de Code Explorer ce que Claude Code et Codex lisent avant d'agir : une compétence, un
simple dossier à copier dans `.claude/skills/` ou `.codex/skills/`.

Elle dit à l'agent trois choses :
• quand cartographier le dépôt avec Code Explorer avant de chercher à la main (status, analyze, context, impact) ;
• quand envelopper une commande bruyante avec lm-resizer, toujours avec `--raw-on-failure` : une commande qui échoue s'affiche brute, jamais résumée ;
• comment retrouver la sortie complète quand il en a besoin, et comment rendre compte de ce qu'il a économisé, mesuré, pas estimé.

Pas de hook obligatoire, pas de serveur MCP obligatoire. Un dossier, et l'agent sait.

Vous n'êtes pas sur Claude Code ? `npx skills add phuetz/lm-resizer` installe la même compétence pour Codex, Gemini CLI, Copilot, Hermes, OpenClaw et cinq autres, vérifié le 10/09.
Et pour Claude Code, une seule commande installe la compétence et le serveur MCP :
`claude plugin marketplace add phuetz/lm-resizer` puis `claude plugin install lm-resizer@phuetz-tools`

github.com/phuetz/lm-resizer

#IA #ClaudeCode #Codex #AgentsIA #Rust #DeveloperTools

## English (short)

A tool your agent doesn't know when to use is useless. lm-resizer and Code Explorer now ship drop-in skills for Claude Code and Codex:
one folder to copy into `.claude/skills/` or `.codex/skills/`. It tells the agent when to map the repo first, when to wrap a noisy
command (always `--raw-on-failure`: a failing command is never summarised away), how to recover the raw output, and how to report
measured savings. Not on Claude Code? `npx skills add phuetz/lm-resizer` installs the same skill for Codex, Gemini CLI, Copilot, Hermes, OpenClaw and five more (verified). Claude Code users: `claude plugin marketplace add phuetz/lm-resizer && claude plugin install lm-resizer@phuetz-tools` installs the skill and the MCP server at once. github.com/phuetz/lm-resizer

#AI #ClaudeCode #Codex #DevTools

Checklist : `npx skills add` vérifié le 10/09 sur les deux dépôts (1 compétence trouvée, installée dans `.agents/skills/`) ; le plugin Claude Code vérifié en installation locale et distante.
