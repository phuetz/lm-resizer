# Changelog

## Non publié

### Changé
- **Licence : Apache-2.0 → Business Source License 1.1.** Usage personnel, non
  commercial et interne à une entreprise autorisés ; ce que la licence retient,
  c'est le fait d'offrir lm-resizer lui-même comme service à des tiers. Bascule
  automatique vers Apache-2.0 le 31 août 2030. Paramètres alignés sur ceux de
  Code Explorer, du même auteur.
- Ce changement **n'est pas rétroactif** : les versions publiées jusqu'à la
  0.2.2 incluse restent sous Apache-2.0, et quiconque les a obtenues conserve
  les droits accordés. La bascule vaut à partir de la version suivante.
- Aucune dépendance ne s'y oppose : audit des 340 caisses du verrou, deux seules
  licences non permissives — `option-ext` en MPL-2.0 (copyleft par fichier, non
  modifié) et `r-efi` qui laisse le choix de MIT — toutes deux compatibles avec
  une distribution sous BUSL.
- Le modèle `standard_v3_3` embarqué reste sous les termes Apache-2.0 de Google.

## [0.2.2] - 2026-09-16

### Added
- Skill Grok « lm-resizer » (`skills/grok/lm-resizer/`) : déclenchement contextuel quand une tâche bénéficie d'une compression locale (`compress -i`), sans envelopper chaque commande.
- Installateur idempotent `scripts/install-grok-skill.sh` (`--target grok|codex`, `--dest DIR`, `--force` avec sauvegarde horodatée) : préserve les fichiers personnalisés, répare un fichier manquant, refuse les arguments inconnus (exit 2) ; tests `scripts/test-install-grok-skill.sh`.
- Auteur des skills et de l'installateur : Grok 4.6 (revue croisée Claude Opus 5, corrections appliquées).

## [0.2.1] - antérieur
- Voir l'historique git.
