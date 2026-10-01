# Reprise sélective du travail sauvegardé du 07/09

La référence `refs/sauvegardes/lmr-wip-20261001` conserve le travail original
(`e5e8d9c`), soit 11 fichiers et +1 800/−135 lignes depuis son premier parent.
La reprise part de la recette 0.2.4 (`86bf58f`), qui descend de master.

Les changements conservés corrigent des défauts précis :

- SQLite purge les entrées expirées au premier ajout, tous les 64 ajouts et
  lors du comptage. Les originaux encore valides restent disponibles même
  au-delà de la capacité du store mémoire. Cette maintenance borne les lignes
  expirées entre deux purges ; elle ne borne ni les données actives ni la taille
  physique du fichier SQLite.
- Les filtres Docker conservent les erreurs, avertissements, échecs, paniques,
  erreurs fatales et exceptions sans dépendre de leur casse.
- Le filtre natif Vitest/Jest précède les règles intégrées générales. Les
  filtres explicitement fournis et ceux du projet approuvés restent prioritaires.
  Une source invalide provoque un avertissement et le retour aux filtres intégrés.
  Un même nom désigne une seule définition : fichier explicite, puis projet,
  puis règles intégrées. Les doublons internes à une source sont refusés.
- `trust-filters` refuse aussi les diagnostics de vérification, notamment les
  doublons et les filtres sans fixture de test.
- `rewrite-shell` conserve à l'octet les expressions contenant une redirection
  ou un pipe, car un autre processus ou un fichier consomme alors leur sortie.
- L'historique masque les formes de secrets reconnues et limite la longueur
  des commandes enregistrées. Les sorties brutes et les rapports JSON peuvent
  toujours contenir des données sensibles : cette règle ne les anonymise pas.
- Le proxy lancé par `run` reçoit la clé fournisseur et l'URL amont dans son
  environnement, via les options existantes, plutôt que dans ses arguments.
  L'environnement reste accessible aux processus disposant des droits requis.

Le contrat CLI/MCP de la 0.2.4 reste utilisable. Code Buddy a été adapté à
`lm_resizer_tool_output` sur MCP ; `tool-output --request-json` n'est donc pas
introduit. Le hash final de `cache_keys` continue d'adresser l'original exact.
Les compteurs utilisent toujours le tokenizer de référence de la 0.2.4.

Les autres propositions restent dans la sauvegarde : budget global en
octets/4, activation générale de SearchOffload, nouveaux filtres `ps`/`du`,
API HTTP `/tool-output`, authentification distante et partage du pipeline/store.
Elles demandent une validation spécifique de la conservation des informations
et des contrats publics. Le plafond SQLite de 1 000 lignes n'est pas repris,
car il supprimerait des originaux avant leur expiration.

Le retrait du compresseur de source ne s'applique pas tel quel au compresseur
syntaxique actuel. Le test de commentaires du WIP passe déjà sur la 0.2.4.
Les scripts exécutables, la récupération CCR complète et la décision du hook
sont déjà corrigés dans master ou la recette. Le durcissement des permissions
Unix du WIP reste partiel : il ne couvre pas les fichiers WAL/SHM SQLite et
modifie les droits de chemins préexistants fournis par l'utilisateur.

La validation comprend des tests rouges sur la 0.2.4 ou sur le constructeur
de proxy avant correction, les tests du workspace et Clippy avec avertissements
interdits. Les preuves détaillées et les limites de plateforme accompagnent le
rapport de livraison ; aucune publication n'est effectuée par cette reprise.
