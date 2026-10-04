# Petites copies : seuils renforcés

Fenêtres normalisées : rejet dès **56** jetons, contre 72. Corps complets : dès **12** jetons, avec conservation des noms des API appelées et espaces de noms (les paramètres et le nom englobant restent normalisés).

[Mutant](guard-mutant.json.gz) : le corps de trois lignes de `tokenize_git_log_args`, lu depuis l’archive épinglée puis renommé, déclenche un code **1**. Deux correspondances de **15 jetons** dans les wrappers Git amont, aucune avec l’ancienne fenêtre de 32. [Produit](guard-current.json.gz) : code **0**, zéro violation lexicale, de politique ou de petite fonction ; 23 fichiers contre 131, SHA des sources présents. Sept tests passent, dont un contrôle négatif avec un autre appel d’API et un module de tests exclu.

Aucune exemption par fichier. L’absence de faux positifs est mesurée sur cet arbre ; un futur wrapper idiomatique identique peut nécessiter une analyse humaine. Ce n’est pas une preuve universelle d’originalité.

[Banc rejoué](parity-guard.json.gz) : médiane **21,74 %**, moyenne **31,58 %**, 61/61 bruts et codes du producteur. Le binaire produit n’a pas changé dans ce commit. Le code strict 1 reflète toujours les différences natives publiées, pas un défaut de récupération.
