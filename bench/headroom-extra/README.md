# Headroom au-dessus de la parité RTK — troisième reprise

Headroom 0.39.1, Apache-2.0, reste une dépendance du banc. Le produit utilise une implémentation Rust indépendante, sans service résident ni modèle distant. Les ajouts ne s’appliquent qu’aux producteurs hors registre natif ; `cat` conserve sa vue littérale. Ces chiffres sont historiques ; voir [les mesures actualisées](../native/README.md).

Cette reprise **ajoute le rendu CSV pour les tableaux de chaînes** et **la compaction des documents JSON imbriqués**. Le regroupement de journaux et l’outline Rust étaient déjà présents : leurs gains sont vérifiés à nouveau, pas présentés comme de nouveaux ajouts.

| Capture | Brut / RTK | LM avant | LM après, tee compris | Headroom spécialisé | Gain LM | API générale Headroom |
|---|---:|---:|---:|---:|---:|---:|
| python-distributions | 2588 | 1182 | 1088 | 1051 | 57.96 % | 1051 |
| health-log | 10013 | 62 | 62 | 119 | 99.38 % | 10013 |
| rust-source | 7380 | 778 | 778 | 4195 | 89.46 % | 7380 |
| npm-lock | 20771 | 20771 | 16897 | 16893 | 18.65 % | 16897 |

Le CSV gagne **94 jetons supplémentaires** par rapport au rendu LM précédent. Le nouveau JSON npm gagne **3 874 jetons** ; le binaire précédent rendait les 20 771 jetons. Le `package-lock.json` provient de l’installation réelle de vitest 3.2.4 ; il est publié dans `npm-lock.json.gz` avec sa provenance et son empreinte. La minification conserve tous les lexèmes, dont les échappements, nombres et clés dupliquées. Headroom gagne quatre jetons de plus en remplaçant un sous-tableau par une chaîne CSV avec schéma ; le banc décode ce schéma avant de vérifier tous les objets.

| Capture | RTK ms | LM ms | Headroom ms |
|---|---:|---:|---:|
| python-distributions | 31.19 | 32.09 | 825.69 |
| health-log | 28.56 | 26.78 | 112.23 |
| rust-source | 10.41 | 36.69 | 113.44 |
| npm-lock | 23.41 | 51.68 | 147.83 |

Trois processus frais par outil et par cas, démarrage/import compris. Les JSON `results.json` et du banc principal identifient le même binaire final. Les trois captures antérieures sont conservées octet pour octet.
## Ce qui gagne et ce qui n’est pas ajouté

- **JSON** : Headroom SmartCrusher essaie une représentation tabulaire puis peut sélectionner des lignes par importance/variance. LMR factorise les clés en colonnes et garde **toutes** les cellules typées ; le banc reconstruit chaque objet et compare l’ensemble à l’entrée. Le nouveau rendu CSV explicite « toutes cellules de type chaîne », cite son nombre de lignes et utilise les règles de guillemets CSV. Les objets à cellules mixtes gardent la table JSON typée. Le banc reconstruit tous les objets avec le parseur CSV standard Python.
- **Journaux** : l’API générale Headroom donne ici 0 %, tandis que `LogCompressor` donne 98,81 %. LMR regroupe seulement les lignes consécutives strictement identiques, avec leur nombre exact. L’erreur `request_failed request_id=42 reason=database_unavailable` et les deux séries de 500 INFO restent vérifiables.
- **Code** : l’API générale donne ici 0 %, `CodeAwareCompressor` 43,16 %. LMR utilise `syn` pour supprimer des plages AST de corps Rust et garder les signatures originales. Le test couvre Unicode, accolades dans les chaînes et syntaxe invalide. Les constantes et les zones hors corps restent littérales ; la vue se présente explicitement comme un outline. Les corps, y compris leurs éventuelles fonctions internes, sont récupérables par tee.
- **Code Explorer/tree-sitter** : l’intégration externe existante a été examinée. Pour ce gain Rust, `syn` évite un index externe et utilise des positions AST natives. Aucun port des treize langages de Code Explorer n’est annoncé ; aucun autre langage n’est activé sans mesure.
- Déduplication sémantique approximative, cache inter-requêtes, TOIN et sélection d’anomalies ne sont pas ajoutés : ce banc ne fournit pas de preuve de gain/qualité pour eux. Ils sont distincts de la simple compression d’une sortie de commande.

Tout ajout est rejeté s’il augmente les octets ou ne réduit pas les jetons. Le tee est vérifié octet pour octet pour les quatre cas. Les signatures conservées ne prouvent pas l’équivalence sémantique d’un programme amputé de ses corps : ce n’est pas l’objectif d’une vue résumée.

## Reproduction

Après `bench/rtk-parity/replay.sh` (l’écart strict TSC reste publié), avec le Python installé par ce script :

```sh
target/rtk-parity/venv/bin/python bench/real/benchmark_headroom_extra.py \
  --work target/controle-headroom-neuf
```

Le répertoire doit être nouveau. Le script **rejoue le corpus public épinglé**, et le nouveau npm-lock épinglé, sans remplacer ses métadonnées Python par celles d’une nouvelle installation. Il relance les API générales et spécialisées : `headroom_default_tokens` publie aussi les résultats par défaut (1 051 / 10 013 / 7 380 / 16 897), et les quatre réductions LMR sont exigées. `probe_headroom.py` reste l’outil de capture initiale. `results.json` contient le SHA-256 du même binaire LMR que le banc principal, les modes API, les durées et la restitution intégrale. Tokenizer : tiktoken 0.14.0, o200k_base.

Sources : [Headroom](https://github.com/headroomlabs-ai/headroom/tree/v0.39.1), [licence Apache-2.0](https://github.com/headroomlabs-ai/headroom/blob/v0.39.1/LICENSE), [SmartCrusher](https://docs.headroomlabs.ai/docs/smart-crusher), [compresseurs et configuration](https://docs.headroomlabs.ai/docs/how-compression-works). La configuration et les API sont vérifiées dans le paquet 0.39.1 installé, pas seulement dans la documentation courante.
