# Résumés de réussite

Cargo conserve explicitement `0 failed` ; le cas cargo_ok passe de 28 à 24 jetons, rappel compris. Le rendu .NET reconnaît les compteurs cohérents et conserve littéralement toute sortie inconnue ou avertissement : dotnet_ok passe de 54 à 17 jetons (référence : 21). Les bruts sont récupérés par une vraie commande tee read.

[Quatre cas](four-tests.json), [rejeu des 61 captures](parity-tests.json.gz) : médiane 21,74 %, moyenne 31,58 %, 61/61 bruts et statuts producteur. Six tests unitaires de la famille passent. Les diagnostics et les patches sont traités séparément.
