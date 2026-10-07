# Plis réversibles et fonctions de la matrice

Ajout de cette reprise : lettres de lecteur, séparateurs Windows, chemins réseau et espaces dans les plis de chemins et de matches. Les chaînes restent littérales, l’inverse est contrôlé avant publication. Deux sondes annoncées comme synthétiques passent de 1 400 à 426 et de 1 800 à 628 jetons, rappel inclus. Les appels spécialisés Headroom laissent ces deux sondes intactes. Quatre unités passent.

[Rejeu des plis](reversible.json) : les autres chemins/grep restent réversibles ; les lignes index retirées par la vue explicite hors registre se récupèrent par tee, pas par expand. La déduplication de deux messages contrôlés passe de 6 207 à 3 126 jetons (Headroom 3 135), avec reconstruction de chaque message.

[Autres vues existantes](extras.json) : distributions JSON 2 588 → 1 088, journal 10 013 → 56, contour Rust 7 380 → 778, document JSON 20 771 → 16 897. Ce ne sont pas de nouveaux ajouts. La comparaison précise quel appel spécialisé est employé ; l’API générale ne compresse pas tous ces textes.

[Contours existants](outline.json) : Python 275 → 51, TypeScript 274 → 126 ; cat reste littéral, syntaxe invalide inchangée, tee exact. Seuls Python, TypeScript et Rust sont mesurés ici, pas toutes les grammaires disponibles.

La [matrice de 88 commandes](../matrix.md) contient déjà err/test/summary/json/deps/env/format/config/run/proxy/hook-audit et les rappels. Les tests inspection_commands exercent réellement les contrats (arguments, codes, récupération). Les nombreuses commandes uniquement génériques restent signalées comme telles ; aucune parité spécialisée inventée. Les exclusions télémétrie et analyse de sessions restent exclues. Aucun SearchCompressor ni DiffCompressor introduit.

[61 cas après ajout](parity-launch.json.gz) : médiane 22,79 %, moyenne 31,78 %, bruts et codes producteur 61/61 ; égalité stricte des vues 49/61. [Contrôle de similarité](guard-launch.json.gz) : zéro violation.
