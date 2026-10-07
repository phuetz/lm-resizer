# Archive du banc avant migration native

Ce répertoire conserve les mesures des premières livraisons. **Les résultats de migration ci-dessous sont historiques ; la section de construction de l’oracle reste applicable au banc actuel.** Les mesures et limites actuelles figurent dans le [banc natif](../native/patch-revision.md), la [matrice](../native/matrix.md) et le [contre-audit de conception](../native/source-independence.md).

## Résultats historiques sur les 61 captures

| Révision / outil | Médiane économisée, tee compris | Moyenne |
|---|---:|---:|
| Avant comparaison, 48ff72b | 0,00 % | 1,53 % |
| Première livraison 8c72b2b | 0,48 % | 19,54 % |
| Reprise 9ae8c47 | 15,05 % | 30,15 % |
| Ancienne livraison intégrée | 15,81 % | 31,60 % |
| RTK 0.50.0 | 15,81 % | 32,61 % |
| Headroom 0.39.1, API générale | 0,00 % | 1,91 % |

La livraison intégrée comptait 58 égalités strictes, deux adaptations du rappel et une exception TSC. Son contrat 61/61 ne désignait donc pas une égalité stricte de toutes les vues. Le corpus brut et le code du producteur étaient préservés 61/61 ; seuls 52/61 codes étaient identiques à l'oracle. Neuf routes pipe amont retournaient zéro pour un producteur en échec.

La politique d'indication de récupération était mesurée en o200k_base et exigeait 25 % de réduction tee compris. Les faibles réductions restaient récupérables via la liste des archives et les métadonnées JSON. Les durées historiques publiées étaient de 21,24 ms de médiane pour LM, 3,54 ms pour RTK et 501,16 ms pour Headroom (trois répétitions, au moins 64 pour TSC, import Python compris).

Les données détaillées, codes, indications de récupération, échecs et répétitions restent dans [publication.json](publication.json). Les entrées sont dans [corpus.json.gz](corpus.json.gz) ; versions et empreintes dans [reference.json](reference.json). Ces archives ne sont pas retouchées pour leur faire décrire la réécriture native.

## Oracle utilisé par le banc actuel

RTK est exclusivement un exécutable de comparaison, construit hors du produit depuis l'archive amont épinglée. [`build_oracle.py`](../real/build_oracle.py) vérifie le SHA-256 et l'intégralité de l'arbre avant et après compilation. [`parity_rtk.py`](../real/parity_rtk.py) compare les vues sans normalisation, mesure les jetons tee compris et distingue explicitement le statut du producteur de celui de l'oracle. Il reste en échec lorsque des vues diffèrent ; le détail historique des cinq différences est publié dans le banc natif.
