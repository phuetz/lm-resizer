# LM Resizer : Foire Aux Questions

Cette page fournit des réponses factuelles concernant l'intégration, la sécurité et les performances de LM Resizer, basées sur les données et la documentation du dépôt.

### La sécurité des hooks qui réécrivent le shell : sont-ils opt-in ? Que se passe-t-il si l'analyse échoue ?
Oui, les hooks shell sont **strictement opt-in** (activables sur demande). Vous devez lancer manuellement `lm-resizer init-native-hooks` ou `lm-resizer init-shims` pour les activer. Si le gestionnaire de hook rencontre un format d'événement bash inconnu ou ne parvient pas à analyser la ligne de commande, il quitte avec succès sans rien modifier (voir `docs/PORTING.md`). LM Resizer **ne modifie pas** votre shell ou la configuration de votre agent sans votre permission explicite.

### Y a-t-il une perte d'information ? Comment l'oracle la mesure-t-il ?
La perte d'information est strictement évaluée en vérifiant que « l'oracle » (les faits nécessaires pour résoudre la tâche, comme les codes d'erreur, les numéros de ligne et les valeurs attendues/observées) est intégralement conservé. Dans notre banc d'essai (`bench/RAPPORT.md`), une économie n'est qualifiée que si 100,0 % des faits de l'oracle sont préservés. Pour les voies de compression du banc, la sortie brute est conservée localement dans CCR ou un fichier tee. CCR expire après 30 minutes par défaut : exportez le texte récupéré avant expiration. Les fichiers tee n’ont pas le TTL CCR et restent jusqu’à suppression ou purge. La récupération exec est du texte UTF-8, avec stdout/stderr combinés et les octets invalides remplacés ; l'oracle ne vérifie que les faits déclarés pour chaque fixture.

### Pourquoi ne pas utiliser `head`, `tail` ou `--quiet` ?
L'option `--quiet` masque souvent l'erreur même que vous cherchez à corriger. Tronquer avec `head` ou `tail` coupe régulièrement le milieu de la sortie, là où se trouvent le contexte pertinent et les détails du test en échec. LM Resizer utilise des parseurs spécifiques (ex: `cargo-test`, `vitest`) pour extraire exactement les erreurs et le contexte sans les perdre. LM Resizer **ne devine pas** le problème magiquement ; il s'appuie sur une analyse structurée de la sortie.

### Comment avoir confiance dans le binaire ?
L'outil est open-source et peut être compilé localement avec `cargo build --release`. Le dépôt documente l'empreinte SHA-256 exacte du binaire lors des mesures (`bench/RAPPORT.md` documente le rejeu actuel). LM Resizer **ne transmet aucune** donnée de télémétrie distante et n'exécute aucun collecteur en arrière-plan par défaut.

### Quelle est la licence ? M'oblige-t-elle à utiliser Code Explorer ?
LM Resizer est distribué sous licence **Apache-2.0**. L'outil optionnel [Code Explorer](https://github.com/phuetz/code-explorer) utilise une licence BUSL-1.1 (qui passera en Apache-2.0 en 2030). Pour maintenir cette séparation de licences, l'intégration se fait uniquement par appel CLI (`code-explorer cypher --repo ...`), gardant les distributions distinctes (`docs/COMPRESSION-SYNTAXIQUE.md`). Code Explorer est totalement optionnel ; LM Resizer **n'en a pas besoin** pour fonctionner de base.


### La modification du prompt casse-t-elle le cache du fournisseur (prompt caching) ?
Dans le module de sortie autonome (non raccordé au proxy HTTP), pour les requêtes Anthropic, `steer_verbosity` contrôle `frozen_count` et n'injecte rien si le dernier message utilisateur est gelé ; le message système n'est pas modifié. Le dépôt contient des tests hors ligne du cache-control et de la zone active. Le taux réel de succès du cache fournisseur n'a pas été vérifié ici.

### Pourquoi LM Resizer affiche-t-il 0 % d'économie sur le code source ?
Les six petites fixtures de code source du banc restent inchangées sans index de code externe. Cela ne garantit pas le même résultat pour tout fichier source : un fichier long peut donner un résumé structurel approximatif. `compress --input` et `smart` essaient les symboles indexés de Code Explorer lorsqu'ils sont disponibles (`docs/COMPRESSION-SYNTAXIQUE.md`) et reviennent à la compression ordinaire si le conseil est indisponible. Récupérez l'original par CCR quand il vous faut le code complet.

### L'installation se fait-elle en une commande ? Qu'en est-il de Windows ?
Le README donne une commande d'installation pour Linux/macOS et une pour Windows, après publication de la release binaire v0.2.4. Elles vérifient la somme SHA-256 de l'archive et la version du binaire. On peut aussi compiler depuis les sources avec Cargo. `scripts/install-grok-skill.sh` installe une skill facultative, pas le binaire (`README.fr.md`, `docs/RELEASE.md`).
