# Changelog

## [0.2.2] - 2026-09-16

### Added
- Skill Grok « lm-resizer » (`skills/grok/lm-resizer/`) : déclenchement contextuel quand une tâche bénéficie d'une compression locale (`compress -i`), sans envelopper chaque commande.
- Installateur idempotent `scripts/install-grok-skill.sh` (`--target grok|codex`, `--dest DIR`, `--force` avec sauvegarde horodatée) : préserve les fichiers personnalisés, répare un fichier manquant, refuse les arguments inconnus (exit 2) ; tests `scripts/test-install-grok-skill.sh`.
- Auteur des skills et de l'installateur : Grok 4.6 (revue croisée Claude Opus 5, corrections appliquées).

## [0.2.1] - antérieur
- Voir l'historique git.
