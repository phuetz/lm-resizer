# Patches réversibles, version 2

Le mode ordinaire 100644 est implicite dans les nouveaux en-têtes F/FT ; les deux identifiants d’objets et le chemin restent explicites. Les autres modes conservent leur en-tête complet. Le décodeur lit encore les vues v1. Aucun hunk, contexte ou message de commit n’est éliminé. Les directives littérales sont échappées.

Le rendu s’applique désormais aussi à git show lorsque sa sortie contient un patch. Sur de vrais historiques Git construits pour le test, show passe de 4 281 à 130 jetons (lignes répétées) et de 4 922 à 4 897 (lignes distinctes), avec reconstruction et tee exacts. [Preuve](patch-final.json). Cinq tests unitaires passent, modes et tabulations compris.

Le petit cas git_diff passe de 156 à 146 jetons, contre 125 pour la référence : les 21 jetons d’écart restent assumés pour une représentation intégralement réversible. [Quatre cas](four-final.json). Le [rejeu 61 captures](parity-final.json.gz) conserve une médiane ≥21 % et 61/61 bruts et codes producteur. Le [contrôle anti-copie](guard-final.json.gz) renforcé reste sans violation.
