# Barrière de livraison

Binaire de publication SHA-256 `a7fb175fafa01d6b8acaed69184bbd77d54d706629df4a0d18357cbc1f5c35e8`.

- `cargo test --workspace --offline` : 1 355 succès, 3 ignorés, 35 suites.
- `cargo clippy --workspace --all-targets --offline -- -D warnings` : succès. Deux avertissements préexistants corrigés : construction de regex sortie d’une boucle de test, fonction déplacée avant le module de tests. Aucun algorithme changé par ces deux corrections.
- [Surface](surface-delivery.json) : dépendances, chaînes du binaire et 60 aides contrôlées.
- [Similarité](guard-delivery.json.gz) : zéro violation ; sept tests du contrôle réussis. Le mutant de 15 jetons reste documenté dans guard.md.
- [Windows portable](windows-delivery.json) : sept cas réussis sur Linux, pas une recette Windows. Le test Node du repli TLS réussit ; PowerShell réel non exécuté.
- [Grande capture](large-delivery.json) : 11 059 229 octets stdout/stderr entrelacés, tee exact, code 7.
- [Banc 61](parity-delivery.json.gz) : 22,79 % de médiane, 31,78 % de moyenne, tee inclus ; bruts et codes producteur 61/61. Le code strict du banc est 1 (49/61 vues strictement égales), pas un échec de ces contrats.
- [Temps entrelacés](timing-delivery.json.gz), sept répétitions sur la même machine : avant 7,675 ms, après 7,698 ms, concurrent 2,921 ms ; moyennes 13,769 / 13,709 / 7,594 ms. Pas d’accélération revendiquée.

Le test concurrent de grandes écritures imposait à tort une atomicité de 90 Ko. Le pipe local annonce PIPE_BUF=4 096. [Sonde sans LM](pipe-atomicity.json) : avec lecture retardée de 30 ms, des lignes de 106 385/73 617 et 139 153/40 849 octets apparaissent, tout en gardant 90 000 A, 90 000 B et deux LF. Le test corrigé garde ces comptages puis exige l’intégrité et l’ordre par producteur de 1 000 écritures de 1 024 octets. Les tests d’ordre déterministe et de dépassement de 10 Mio restent inchangés.
