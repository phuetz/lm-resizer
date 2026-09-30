# Compression structurelle guidée par Code Explorer

`lm-resizer compress --input fichier.cs` utilise les symboles de l'index Code Explorer lorsqu'il existe. Le même chemin couvre Go, Java, C et C++, après C#. Le binaire appelle `code-explorer cypher --repo ...`, lit les plages de classes et de fonctions du graphe, puis retire seulement les lignes intérieures des corps de fonctions. Il garde les signatures, attributs, imports et accolades. Le fichier complet est conservé dans CCR pendant 30 minutes par défaut (exportez-le avant expiration pour une preuve durable) ; la sortie donne une clé de récupération. Une requête qui nomme une fonction garde son corps entier.

L'index doit contenir le fichier et être au moins aussi récent que lui. Le pont refuse un résultat absent, obsolète ou mal formé ; le pipeline ordinaire prend alors la suite. `--advice-from-code-explorer` demande explicitement la tentative et expose son statut dans `--json`. Pour préparer un projet :

```bash
code-explorer analyze chemin-du-projet
lm-resizer compress --input chemin-du-projet/Service.cs --json
```

Le graphe Code Explorer en version locale 0.2.1 retourne parfois `endLine == startLine` pour les fonctions C et C++. Dans ce seul cas, LM Resizer complète la plage lorsque la ligne de signature contient une accolade ouvrante, avec le même lecteur d'accolades que son contrôle de sécurité. Les déclarations sans corps et les formes ambiguës restent intactes. Une plage non équilibrée ou une compression qui grossirait le résultat est refusée.

LM Resizer est sous Apache-2.0. Le dépôt Code Explorer contient une licence BUSL-1.1 avec changement annoncé vers Apache-2.0 le 31 août 2030. GitHub peut ne pas reconnaître cette licence, mais le fichier `LICENSE` et `Cargo.toml` du dépôt l'indiquent. Nous utilisons son processus CLI plutôt qu'une dépendance Cargo intégrée, afin de garder les distributions et leurs conditions de licence distinctes. Cette séparation n'est pas un avis juridique sur tous les usages commerciaux de Code Explorer.

Le corpus `fixtures/parity/source` et `fixtures/parity/manifest.json` exerce les cinq langages. `cargo run --example parity_bench -- target/debug/lm-resizer` indexe une copie temporaire du corpus, compare les tokens avant et après avec le tokenizer `gpt-4o`, vérifie les oracles et récupère le brut par CCR. La latence affichée inclut le lancement du processus CLI ; elle ne mesure ni un appel fournisseur ni une facture.
