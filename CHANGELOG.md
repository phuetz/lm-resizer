# Changelog

## [0.2.4] - à publier

### Corrigé
- Statistiques CLI : comptage réel du texte final avec `tiktoken-rs/o200k_base` (encodage `o200k_base`), distinct des estimations historiques octets/4 ; les chiffres comparatifs des README étaient déjà tokenisés et ne sont pas remplacés par ces statistiques.
- Parcours source documenté pour PowerShell avec préfixe utilisateur explicite, PATH de session et prérequis MSVC/SDK Windows ; exemples `discover` sans tilde non développé par les commandes natives.
- Intégration de `60d5cc6`, absent du tag v0.2.3 (`44b9ccc`) : la première clé CCR du CLI conserve l'entrée exacte avant minification ou résumé de source. `retrieve` peut ainsi restituer l'original UTF-8 octet pour octet avant expiration du CCR.
- Le même correctif rend les scripts shell générés par les helpers et shims exécutables et fait échouer `discover` sur un chemin explicitement demandé mais absent.
- Versions Cargo, WASM, npm, plugin et installateurs alignées sur 0.2.4 ; commandes README prêtes pour la publication future de v0.2.4. La préparation ne publie aucune archive.
- Test de bout en bout du CLI pour une source et un JSON formaté, avec comparaison des octets récupérés et possibilité de tester un autre binaire.
- Documentation corrigée après audit : TTL CCR de 30 minutes, limites UTF-8 et codes de sortie d'`exec`, configuration Codex globale même avec `all --scope project`, syntaxe de récupération, statistiques réellement disponibles et résultats du banc. Retrait des chiffres attribués à Headroom sans source et de la comparaison MCP sans capture versionnée.
- Les anciennes preuves locales de `dist` ne sont plus suivies : les sommes de contrôle doivent être régénérées à partir des archives effectivement publiées.
- Rust minimal déclaré à 1.86 pour aligner le guide avec les dépendances ; la validation locale utilise la toolchain indiquée dans le rapport de préparation.
- Deux avertissements Clippy corrigés sans changement voulu de comportement : argument fixture/code de sortie regroupé dans le banc et simplification du contrôle de budget JSON.

### Depuis v0.2.3
- Les archives binaires ont été allégées (`a339c16`, intégré par `0fef336`) : binaire, documentation et skills utiles à l'utilisateur, sans le matériel de développement du dépôt.

## [0.2.3] - publiée avant cette préparation

### Added
- Installateurs binaires en une commande pour Linux, macOS et Windows, avec vérification SHA-256 des archives de release.
- Test CI de l'installation Linux sur un runner neuf sans Rust et préparation d'une release avec archives.

## [0.2.2] - 2026-09-16

### Added
- Skill Grok « lm-resizer » (`skills/grok/lm-resizer/`) : déclenchement contextuel quand une tâche bénéficie d'une compression locale (`compress -i`), sans envelopper chaque commande.
- Installateur idempotent `scripts/install-grok-skill.sh` (`--target grok|codex`, `--dest DIR`, `--force` avec sauvegarde horodatée) : préserve les fichiers personnalisés, répare un fichier manquant, refuse les arguments inconnus (exit 2) ; tests `scripts/test-install-grok-skill.sh`.
- Auteur des skills et de l'installateur : Grok 4.6 (revue croisée Claude Opus 5, corrections appliquées).

## [0.2.1] - antérieur
- Voir l'historique git.
