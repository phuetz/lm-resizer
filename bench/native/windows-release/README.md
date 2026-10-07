# Préparation Windows — première étape

[Résultat final de cette reprise](delivery.md). Les chiffres ci-dessous identifient la première étape.

Les octets CP850 et CRLF et les grands échecs sont déjà préservés par la branche native. Le [replay portable](contracts-final.json) confirme aussi le nombre de tests npm et le diagnostic de runner absent. Il s’exécute sous Linux : **ce n’est pas une recette Windows réelle**.

Défaut restant corrigé : le hash dans la vue CCR désignait encore un intermédiaire (1 997 octets pour 25 415 octets d’entrée). Toutes les références affichées et les clés du rapport renvoient désormais l’entrée avant compression, y compris avant filtrage de commande. Tests `cli_retrieval` avec appels réels à `retrieve`. Le binaire précédent échoue au nouveau contrôle sur `intermediate CCR payload`.

Installeur : SHA-256 et ZIP par .NET, sans Get-FileHash ni module Archive ; TLS 1.2 et repli Node avec validation de certificat. Le test Node vérifie les octets, statuts et redirections, pas Schannel. Le test PowerShell existant ajoute un Get-FileHash indisponible et un PSModulePath invalide. Il reste à exécuter sur Windows 5.1/7 ; aucun PowerShell ni Wine disponible sur cet hôte. [Procédure Windows](../../../docs/WINDOWS.md).

Hooks : champ `cmd` conservé et réécrit, appel PowerShell par `&`, commande de hook Windows avec hôte PowerShell explicite ; tests JSON, sans conversation agent authentifiée.

[61 cas](parity-windows.json.gz) : médiane **21,74 %**, moyenne **31,58 %**, tee compris, **61/61 bruts et codes producteur**. Le banc strict reste en code 1 pour les différences natives déjà publiées. Aucun gain Headroom revendiqué dans cette étape. [Surface du binaire](surface-windows.json) : chaînes, dépendances et 60 aides propres. Tests workspace : 1 350 succès, 3 ignorés ; quatre tests ciblés de récupération passent après le dernier renforcement. Clippy workspace et formatage passent.
