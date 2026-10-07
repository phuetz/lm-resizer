# Faits visibles : contre-revue du 02/10/2026

Ces mesures remplacent les annonces fondées sur le décodage des vues. L’oracle lit la **vue brute**, sans `expand`, sans dictionnaire et sans résolution de références. Les anciens JSON restent des archives, pas des preuves de faits visibles.

## Même corpus historique, nouvel oracle

30 cas principaux sur trois dépôts épinglés; cinq comparaisons supplémentaires exclues de la médiane. Économies signées, non pondérées, sur le texte réellement affiché, instructions tee et avertissement d’ordre des flux inclus. Python tiktoken 0.14.0.

| Encodage | Statistique | Ancienne vue | Vue corrigée | RTK 0.50.0 |
|---|---|---:|---:|---:|
| cl100k_base | médiane | 7.38 % | 0.00 % | 43.28 % |
| cl100k_base | moyenne | 18.34 % | 1.49 % | 47.33 % |
| o200k_base | médiane | 7.38 % | 0.00 % | 43.21 % |
| o200k_base | moyenne | 18.47 % | 1.49 % | 47.41 % |

| Contrôle sur 35 cas | Ancienne vue | Vue corrigée | RTK |
|---|---:|---:|---:|
| Faits exigés | 160 014 | 160 014 | 160 014 |
| Faits non reconnus dans la vue | 146,106 | 0 | 159,131 |
| Codes de sortie conservés | 35/35 | 35/35 | non comparable en mode pipe |
| Comptes CLI o200k = Python | 35/35 | 35/35 | non testé |
| Tee vérifiés parmi les cas avec tee | 25/25 | 1/1 | non testé |
| Cas sans tee | 10 | 34 | non testé |

RTK a été **relancé** : 21 captures identiques via `pipe`, 14 commandes exécutées de nouveau. Les modes sont indiqués dans `results/rtk.json`. Les économies RTK sont nettement supérieures, avec de très nombreux faits non reconnus juste à côté. Ce compteur strict peut aussi refléter une reformulation non prise en charge : ce n’est pas une mesure universelle des pertes sémantiques. La parité d’économie n’est pas atteinte.

## Vrais échecs ajoutés

11 nouvelles commandes isolées : TypeScript (TS2322 ligne 179 et TS2304 ligne 181), ESLint (3 erreurs), Jest (3 assertions échouées, 1 réussie), pytest et Cargo (2 assertions échouées et 1 réussie chacun), grep avec numéros, ls avec taille 12030, cat Python avec indentation/tilde, Git (12 commits, 3 auteurs), Docker logs et build.

Docker utilise une image BusyBox épinglée par digest. Le conteneur fonctionne au premier plan, produit deux erreurs `cat` réelles puis reçoit SIGKILL via son shell (code inspecté 137; aucune prétention de panne OOM). `docker logs` sort normalement en 0. Le build échoue réellement dans `RUN cat`, après préparation réussie. Les diagnostics attendus sont contrôlés dès la capture pour ne pas confondre une panne d’installation avec un échec de test. Le conteneur créé est supprimé.

**186/186 faits visibles, 11/11 codes conservés, 11/11 comptes CLI concordants. Aucun tee : contenu conservé.** Médiane 0 %; moyenne cl100k −4,63 %, o200k −4,85 %. Les instructions sur l’ordre des flux coûtent des jetons, surtout sur les petites sorties.

Ensemble explicite de **41 cas** (30 historiques + 11 nouveaux, cinq comparaisons toujours exclues) : médiane **0 %** dans les deux encodages; moyenne cl100k **−0,15 %**, o200k **−0,21 %**. RTK n’a pas été mesuré sur les 11 nouveaux cas : pas de médiane RTK prétendument comparable sur 41 cas.

## Détail historique o200k

| Cas | Brut | Ancienne vue | Vue corrigée | RTK | Faits absents corrigés | Temps corrigé s |
|---|---:|---:|---:|---:|---:|---:|
| ripgrep-log | 2973 | 2803 | 2973 | 101 | 0 | 0.063 |
| ripgrep-diff | 228 | 228 | 228 | 156 | 0 | 0.056 |
| ripgrep-status | 23 | 23 | 23 | 22 | 0 | 0.044 |
| ripgrep-show | 104 | 104 | 104 | 104 | 0 | 0.042 |
| ripgrep-ls | 837 | 637 | 837 | 245 | 0 | 0.056 |
| ripgrep-recursive | 1639 | 1595 | 1639 | 285 | 0 | 0.057 |
| ripgrep-find | 3695 | 2706 | 3695 | 596 | 0 | 0.046 |
| ripgrep-grep | 243 | 243 | 243 | 243 | 0 | 0.058 |
| ripgrep-cat | 63937 | 48174 | 63937 | 63937 | 0 | 0.067 |
| ripgrep-test | 5081 | 318 | 348 | 232 | 0 | 0.044 |
| fastapi-log | 2172 | 1346 | 2172 | 88 | 0 | 0.049 |
| fastapi-diff | 279 | 279 | 279 | 237 | 0 | 0.045 |
| fastapi-status | 23 | 23 | 23 | 22 | 0 | 0.059 |
| fastapi-show | 107 | 107 | 107 | 107 | 0 | 0.056 |
| fastapi-ls | 529 | 487 | 529 | 136 | 0 | 0.054 |
| fastapi-recursive | 35310 | 21292 | 35310 | 137 | 0 | 0.045 |
| fastapi-find | 61723 | 42243 | 61723 | 622 | 0 | 0.051 |
| fastapi-grep | 759 | 708 | 759 | 722 | 0 | 0.061 |
| fastapi-cat | 223103 | 167588 | 223103 | 223103 | 0 | 0.063 |
| fastapi-test | 100772 | 26232 | 100772 | 40 | 0 | 0.056 |
| TypeScript-log | 1628 | 1517 | 1628 | 88 | 0 | 0.053 |
| TypeScript-diff | 2174 | 1852 | 2174 | 1805 | 0 | 0.046 |
| TypeScript-status | 23 | 23 | 23 | 22 | 0 | 0.049 |
| TypeScript-show | 205 | 205 | 205 | 205 | 0 | 0.061 |
| TypeScript-ls | 745 | 601 | 745 | 237 | 0 | 0.042 |
| TypeScript-recursive | 616572 | 397296 | 616572 | 161 | 0 | 0.086 |
| TypeScript-find | 1361069 | 587128 | 1361069 | 601 | 0 | 0.158 |
| TypeScript-grep | 25868 | 21198 | 25868 | 19347 | 0 | 0.055 |
| TypeScript-cat | 597267 | 583734 | 597267 | 597267 | 0 | 0.099 |
| TypeScript-test | 31 | 31 | 46 | 14 | 0 | 0.059 |
| compare-log | 3083 | 2905 | 3083 | 75 | 0 | 0.048 |
| compare-diff | 43405 | 37676 | 43405 | 2539 | 0 | 0.061 |
| compare-grep | 2966 | 2645 | 2981 | 1795 | 0 | 0.042 |
| compare-find | 14203 | 9905 | 14203 | 775 | 0 | 0.041 |
| compare-cat | 535949 | 474299 | 535949 | 535949 | 0 | 0.088 |

## Performance et provenance

Sources du binaire mesuré : `440826bef55aed4f4fff898773a02dbd325a8b77`, sources propres lors de `cargo build --release --locked`. SHA-256 : `34fe6e4980df4debe8fadecd13214203aefcbf925d79af463073d53dc2db78ce`. Les sources hachées sont incluses dans `results/after.json`. Les modifications suivantes de tests, banc et documentation ne changent pas le code produit.

Vrai `ls -R` TypeScript, cinq paires natives/enveloppées : médiane enveloppée **0.150 s**, maximum **0.185 s**, sortie littérale identique, parcours disque compris. Cache disque non vidé. Aucun seuil de 0,300 s pour cat n’est annoncé comme garanti.

Longue ligne homogène, un essai par taille et mode :

| Octets | tool-output s | exec s | Vue littérale |
|---:|---:|---:|---|
| 250000 | 0.165 | 0.126 | oui |
| 500000 | 0.153 | 0.170 | oui |
| 1000000 | 0.459 | 0.374 | oui |
| 1048576 | 0.375 | 0.390 | oui |
| 20000000 | 9.813 | 9.172 | oui |

Le contre-rapport mesurait 405 s pour un million de caractères; cette ancienne durée n’a pas été relancée ici. La nouvelle file de priorité BPE reproduit l’ordre de fusion global (rang, puis position), sans découpage approximatif. Tests différentiels contre `byte_pair_split`, dont égalités de rang et séquences UTF-8. Le comptage Python sur la ligne entière de 20 Mo n’a pas été attendu : les mesures pathologiques sont des tests de latence et de vue littérale, pas une nouvelle preuve différentielle sur cette taille.

## Mutations et limites

`test_oracle.py` contient les quatre mutations B1–B4 : nombre coupé, auteur en référence, chemin/identifiant en dictionnaire, erreur répétée en référence. Chaque mutation est réversible avec l’ancien décodeur, passe l’ancien principe de contrôle, mais échoue au contrôle visible. Restaurer le décodage dans une copie de l’oracle fait échouer exactement ces quatre tests (10 tests exécutés, 4 échecs). Un échange d’auteurs entre commits est aussi rejeté malgré un multiensemble global identique.

`exec_streams` vérifie les octets non UTF-8 dans tee et `tee read`, en capture normale et `--stream`. La vue signale explicitement les octets invalides par `\xNN`. Les flux sont stockés dans leur ordre de capture séparé, avec un séparateur; leur chronologie commune n’est pas reconstruite et la vue le dit.

La lisibilité n’a pas été évaluée par un modèle externe. Les oracles ne prouvent ni tous les formats futurs, ni Windows/macOS. Les économies sur les familles rendues littéralement sont nulles. Les filtres dédiés aux réussites Cargo/pytest restent actifs; toute syntaxe inconnue reste brute. Aucun chiffre de facture API ni promesse universelle.

Les quatre entrées originales de la contre-revue ont également été rejouées : égalité littérale dans les quatre cas (`results/review-cases.json`). Ce sont les sorties de démonstration du relecteur, distinctes des 11 nouvelles exécutions réelles.
