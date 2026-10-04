# Reprise de conception des vues natives

Le contre-audit a identifié de vraies transcriptions dans la livraison précédente. Elles ne sont pas défendues au motif que les sorties étaient bonnes. Cette reprise supprime les routes mortes et change les modèles de traitement ; elle ne se limite pas à renommer les fonctions.

| Passage | Traitement |
|---|---|
| Ancien `hunk_counts` / `parse_hunk_header` | Supprimé. `patch_view` utilise maintenant un codec réversible : égalité des quatre lignes d’en-tête, répétitions exactes, échappement des directives, inverse. Aucun analyseur de plage ni budget de troncature. La première réécriture décrite dans la livraison précédente a été rejetée. |
| Anciens `filter_git_status`, `filter_cargo_test` | Supprimés, ainsi que cinq autres filtres inaccessibles derrière le registre actif : log, show, TypeScript, pytest, recherche. Les régressions sur les chemins, positions et échecs exercent maintenant le code actif. |
| Vue de log | Extrait calculé en un passage avec un état de présentation borné ; plus de collecte de tout le corps puis troncature. Un historique contenant des patches utilise ce codec et se reconstruit intégralement avec `expand`. |
| Regroupement grep/find | Une liste de records est triée de façon stable et découpée en groupes contigus ; une seule présentation traite chemins et correspondances. Plus de reproduction des deux wrappers amont avec une table associative différente. |
| Calendrier / dates TRX | Conversion par cycles calendaires puis parcours des années et mois ; inverse par cumul de jours et comptage des années bissextiles. Abandon de la transformation algébrique mars/février commune à l'ancien code et à l'oracle. |
| Lecteur binaire | Entiers variables : recherche bornée du terminateur puis évaluation des chiffres en sens inverse, avec rejet des débordements. En-têtes : interpréteur de cellules piloté par un schéma de format. Constantes et ordre des champs sont imposés par MSBuild. |
| Tailles, liste d'échecs, terminaison des processus | Sélection d'unité par ordre de grandeur binaire ; projection d'une tranche bornée pour les échecs ; composition des statuts optionnels. Les travailleurs de capture sont joints dans une portée commune. |

## Barrière reproductible

`python3 bench/real/check_source_similarity.py` lit **l'archive** RTK 0.50.0, après contrôle de son SHA-256 épinglé dans `bench/rtk-parity/reference.json`. Aucun arbre amont modifiable ne sert de référence. L'archive doit avoir été préparée par `bench/real/build_oracle.py`. La barrière est appelée par `scripts/check-release.sh` avec ses sept tests de détection.

Toutes les fonctions des fichiers Rust de `src/` sont examinées, même après un module de tests. Les corps de tests et les déclarations sont exclus : des entrées et sorties de test communes sont autorisées. Les commentaires disparaissent, identifiants locaux, nombres et chaînes deviennent des jetons génériques ; les méthodes et mots-clefs Rust restent significatifs. Fenêtre de recherche : 32 jetons ; rejet à **56 jetons contigus**. Aucun fichier du produit n'est exempté. Le rapport liste aussi les rapprochements sous le seuil, pour relecture.

La barrière rejette le point de départ `af0b54c` : 118 jetons pour le calendrier, 106 pour le parseur de hunk (82 pour sa fonction imbriquée), 78 pour la présentation des échecs. Le test de mutation reprend le passage de hunk **depuis l'archive**, renomme ses identifiants et vérifie qu'il est encore rejeté. Autres tests : commentaires/littéraux modifiés, fonction située après un module de tests, exclusions limitées aux tests/déclarations, refus d'une archive altérée.

Les [résultats avant](similarity-before.json) et [après](similarity-after.json) contiennent les emplacements comparés. Les rapprochements courts ont été relus par paire : listes de mots-clefs différentes, création d'un répertoire parent, lecture et décodage JSON, initialisation de collections, parcours de lignes et regex paresseuses. Le plus long restant est une chaîne de tests `contains` (Docker contre classification de logs), avec des critères différents. Les noms de fonctions communs sont surtout `run`, `new`, `filter` ; l'en-tête binaire au nom et à l'enchaînement communs a été réécrit même sans alarme longue. Les messages longs communs restants sont des sorties attendues, des champs de protocole, un namespace XML et un diagnostic usuel de répertoire personnel.

Un balayage supplémentaire du cœur et de WASM est archivé : [cœur](similarity-core.json), [WASM](similarity-wasm.json). Deux alarmes dans le cœur comparent des chaînes `starts_with` de détection de langage/commentaires à une liste de préfixes de CI GitLab. Les valeurs et les traitements diffèrent : `pub fn`, `impl`, `//` dans le cœur contre `Running with gitlab-runner`, `Using Docker executor` dans l'oracle. Ce sont des collisions de normalisation, conservées dans le rapport, pas des exemptions ajoutées au contrôle de `src/`.

**Limite :** cette analyse lexicale est une alarme de réintroduction, pas une preuve mathématique d'absence de toute parenté, ni un audit juridique. Une réorganisation complexe peut échapper aux fenêtres ; la comparaison des modèles, appels, constantes et messages reste nécessaire. Les ressemblances imposées par les protocoles et le comportement compatible ne prouvent pas une copie.

## Détection de politiques réparties

La [reprise suivante](patch-revision.md) ajoute une signature de sélection au niveau fichier : limites numériques effectivement utilisées dans les comparaisons et take/skip/min/max, messages d’omission locaux et globaux, dont au moins un commun. Elle rejette le `patch_view.rs` de `1d9b48d`, qui échappait aux fenêtres lexicales. Les empreintes de chaque fichier local et de l’archive accompagnent les rapports. Ce détecteur ciblé ne prouve pas l’absence de toute paraphrase.

## Patches et validation

Le test d'intégration `git_history_with_patches_keeps_each_commit_and_its_complete_patch` crée deux commits, exige une réduction de taille en direct et via `exec`, reconstruit avec `expand`, puis compare aux octets de `git log -p` et relit `tee read`. Le banc dédié exige aussi un gain de jetons, sur deux historiques distincts (répétitions et lignes uniques). Les tests du codec couvrent CRLF/LF, fin sans saut de ligne, chemins avec espaces et Unicode, directives littérales, plus de 200 lignes uniques et plus de 20 lignes de contexte, et rejettent les répétitions malformées ou démesurées. Les tests des dates et du lecteur binaire restent inchangés.

L'historique antérieur n'a pas été réécrit. Sa reconstruction avant publication appartient au pilote.


## Petites fonctions complètes

La barrière compare également les corps complets dès **12 jetons**, en conservant les appels d’API et espaces de noms tout en normalisant les variables locales. La fenêtre générale reste de 32 jetons avec seuil abaissé à 56. Le mutant de trois lignes `tokenize_git_log_args`, renommé, est rejeté (15 jetons) ; les variantes invoquant une autre API ne le sont pas. Zéro alarme sur les 23 fichiers actuels de `src/`, face aux 131 fichiers Rust de l’archive vérifiée. Cela mesure l’absence de faux positifs sur cet arbre, pas sur tout programme Rust possible. [Preuves de cette tranche](windows-release/guard.md).
