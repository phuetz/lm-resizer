# LM Resizer : Foire Aux Questions

Cette page fournit des réponses factuelles concernant l'intégration, la sécurité et les performances de LM Resizer, basées sur les données et la documentation du dépôt.

### La sécurité des hooks qui réécrivent le shell : sont-ils opt-in ? Que se passe-t-il si l'analyse échoue ?
Oui, les hooks shell sont **strictement opt-in** (activables sur demande). Vous devez lancer manuellement `lm-resizer init-native-hooks` ou `lm-resizer init-shims` pour les activer. Si le gestionnaire de hook rencontre un format d'événement bash inconnu ou ne parvient pas à analyser la ligne de commande, il quitte avec succès sans rien modifier (voir `docs/PORTING.md`). LM Resizer **ne modifie pas** votre shell ou la configuration de votre agent sans votre permission explicite.

### Y a-t-il une perte d'information ? Comment l'oracle la mesure-t-il ?
La perte d'information est strictement évaluée en vérifiant que « l'oracle » (les faits nécessaires pour résoudre la tâche, comme les codes d'erreur, les numéros de ligne et les valeurs attendues/observées) est intégralement conservé. Dans notre banc d'essai (`bench/RAPPORT.md`), une économie n'est qualifiée que si 100,0 % des faits de l'oracle sont préservés. Pour les voies de compression du banc, la sortie brute est conservée localement dans CCR ou un fichier tee. Elle reste soumise à la rétention locale et aux commandes de purge explicites ; l'oracle ne vérifie que les faits déclarés pour chaque fixture.

### Pourquoi ne pas utiliser `head`, `tail` ou `--quiet` ?
L'option `--quiet` masque souvent l'erreur même que vous cherchez à corriger. Tronquer avec `head` ou `tail` coupe régulièrement le milieu de la sortie, là où se trouvent le contexte pertinent et les détails du test en échec. LM Resizer utilise des parseurs spécifiques (ex: `cargo-test`, `vitest`) pour extraire exactement les erreurs et le contexte sans les perdre. LM Resizer **ne devine pas** le problème magiquement ; il s'appuie sur une analyse structurée de la sortie.

### Comment avoir confiance dans le binaire ?
L'outil est open-source et peut être compilé localement avec `cargo build --release`. Le dépôt documente l'empreinte SHA-256 exacte du binaire lors des mesures (`3336ada76584df5c7ce714e0ccc6c1c404793b8b1f61e4770fe24994335e3f01` dans `bench/RAPPORT.md`). LM Resizer **ne transmet aucune** donnée de télémétrie distante et n'exécute aucun collecteur en arrière-plan par défaut.

### Quelle est la licence ? M'oblige-t-elle à utiliser Code Explorer ?
LM Resizer est distribué sous licence **Apache-2.0**. L'outil optionnel [Code Explorer](https://github.com/phuetz/code-explorer) utilise une licence BUSL-1.1 (qui passera en Apache-2.0 en 2030). Pour maintenir cette séparation de licences, l'intégration se fait uniquement par appel CLI (`code-explorer cypher --repo ...`), gardant les distributions distinctes (`docs/COMPRESSION-SYNTAXIQUE.md`). Code Explorer est totalement optionnel ; LM Resizer **n'en a pas besoin** pour fonctionner de base.

### RTK n'existe-t-il pas déjà ?
Bien que les deux outils compressent le contexte des agents, LM Resizer préserve un oracle beaucoup plus complet dans de nombreux cas. Par exemple, sur `git_log`, LM Resizer conserve 100 % de l'oracle contre 4 % pour RTK ; sur `json_large`, 100 % contre 33 %. Il gagne aussi sur des outils comme `docker` et `psql` où RTK n'a aucun filtre (`bench/resultats.json`). Toutefois, LM Resizer reste derrière RTK sur trois cas spécifiques : `dotnet_ok`, `git_diff` et `compile_error`.

### Headroom annonce 60–95 % d'économie. Pourquoi choisir LM Resizer ?
Sur notre corpus de 22 fixtures synthétiques et réelles, Headroom a obtenu une économie qualifiée médiane de 0,0 %, avec une latence médiane de 430 ms (contre 17 ms pour LM Resizer). De plus, Headroom n'a eu recours à son détecteur Python que 0 fois sur 22 (`bench/RAPPORT.md`). Comme précisé dans `docs/headroom-rtk-2026-09-22.md`, leurs taux annoncés n'ont pas pu être reproduits sur ce corpus.

### La modification du prompt casse-t-elle le cache du fournisseur (prompt caching) ?
Pour les requêtes Anthropic, `steer_verbosity` contrôle `frozen_count` et n'injecte rien si le dernier message utilisateur est gelé ; le message système n'est pas modifié. Le dépôt contient des tests hors ligne du cache-control et de la zone active. Le taux réel de succès du cache fournisseur n'a pas été vérifié ici.

### Pourquoi LM Resizer affiche-t-il 0 % d'économie sur le code source ?
Dans nos mesures, sans index de code externe, LM Resizer applique judicieusement une économie de 0,0 % sur les fichiers sources (C#, Rust, Python, TypeScript, Go, Java). Pour compresser structurellement du code source (garder les signatures mais vider le corps des fonctions), Code Explorer **doit** être installé et le projet indexé (`docs/COMPRESSION-SYNTAXIQUE.md`). LM Resizer **ne tronque pas** aveuglément les lignes de code source sans certitude structurelle.

### L'installation se fait-elle en une commande ? Qu'en est-il de Windows ?
Le README donne deux commandes pour le plugin Claude : `claude plugin marketplace add phuetz/lm-resizer`, puis `claude plugin install lm-resizer@phuetz-tools`. `scripts/install-grok-skill.sh` installe la skill facultative Grok ou Codex, pas le binaire ni le plugin. Windows dispose de scripts PowerShell de livraison et de contrôles CI (`README.md`, `docs/PORTING.md`).
