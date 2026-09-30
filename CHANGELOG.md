# Changelog

## [0.2.3] - à publier

### Added
- Mesure A/B des jetons de session via `measure-session`, fondée sur les compteurs fournisseur (entrée, sortie et cache), avec rejeu Bash séparé et refus des journaux incomplets.
- Installateurs binaires en une commande pour Linux, macOS et Windows, avec vérification SHA-256 des archives de release.
- Test CI de l'installation Linux sur un runner neuf sans Rust et préparation d'une release avec archives.

## [0.2.2] - 2026-09-16

### Added
- Skill Grok « lm-resizer » (`skills/grok/lm-resizer/`) : déclenchement contextuel quand une tâche bénéficie d'une compression locale (`compress -i`), sans envelopper chaque commande.
- Installateur idempotent `scripts/install-grok-skill.sh` (`--target grok|codex`, `--dest DIR`, `--force` avec sauvegarde horodatée) : préserve les fichiers personnalisés, répare un fichier manquant, refuse les arguments inconnus (exit 2) ; tests `scripts/test-install-grok-skill.sh`.
- Auteur des skills et de l'installateur : Grok 4.6 (revue croisée Claude Opus 5, corrections appliquées).

## [0.2.1] - antérieur
- Voir l'historique git.
