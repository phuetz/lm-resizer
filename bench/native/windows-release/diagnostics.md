# Diagnostic de compilation

Le cas compile_error passe de 106 à 86 jetons, contre 78 pour la référence. Le diagnostic, le chemin, les repères et le bilan du producteur restent textuellement présents. Les 8 jetons supplémentaires ne sont pas une erreur : la référence remplace le bilan avec nom du crate et cible par un compte générique. LM conserve cette information. Seuls les gouttières vides et le conseil générique rustc --explain sont retirés.

[Mesure ciblée](four-diagnostics.json), [61 captures](parity-diagnostics.json.gz) : médiane 21,74 %, bruts et codes producteur 61/61. Quatre tests de diagnostics passent, dont conservation ligne par ligne des informations utiles et d’un avertissement ajouté.
