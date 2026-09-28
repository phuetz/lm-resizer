# Banc comparatif côté à côte

Depuis un checkout Linux de LM Resizer :

```bash
./bench/rejouer.sh /chemin/de/sortie/RAPPORT.md
```

Le script installe RTK 0.50.0 (archive vérifiée par SHA-256) et Headroom 0.39.1 avec ONNX Runtime 1.24.4 uniquement sous `target/banc/`, puis compile et lance le harnais Rust. `target/` est ignoré par git et exclu du test `repository_has_no_python_runtime_surface`. Aucun fichier Python n'est versionné ; la fixture de code Python est conservée comme texte et copiée sous `target/banc/fixtures/` avec son extension naturelle au moment du rejeu. L'adaptateur Headroom dans `headroom_once.sh` utilise l'API publique par l'interpréteur isolé, sans script Python dans le dépôt.

`LM_RESIZER_BIN=/chemin/vers/lm-resizer` sélectionne un binaire d'une autre révision. Pour mesurer le code d'une autre révision, exécuter le banc depuis son checkout. Le corpus et ses oracles sont versionnés dans `cases.json` et `corpus/`. Les 22 cas couvrent tests, git, Docker, PostgreSQL, journaux, JSON, erreurs de compilation, six langages et prose. Les fixtures sont synthétiques et ne contiennent pas de données personnelles.

Le harnais compare les mêmes octets avec `o200k_base` de `tiktoken-rs`. LM Resizer utilise `exec` ou `compress --input`, RTK les filtres ou commandes décrits par cas, et Headroom `compress(messages)`. Les commandes sont simulées par des exécutables qui rejouent la fixture et son code de sortie. Le chemin du checkout, HOME et l'horodatage de la ligne tee sont normalisés dans les sorties comptées ; les sorties brutes et leurs empreintes restent sous `target/banc/results/`. La latence inclut le lancement de chaque processus. Une économie avec oracle incomplet vaut zéro ; un gain nul n'a pas de gagnant. Les détails des 66 mesures sont dans `resultats.json`.

Le cas Git exige les 46 sujets et un SHA abrégé qui identifie uniquement le commit visé parmi ceux de la fixture. Le cas JSON vérifie les 180 lignes, l'anomalie et trois lignes témoins, y compris avec la table compacte de SmartCrusher. Les tests Rust du harnais protègent ces invariants. La tâche agent reste non évaluée : l'essai antérieur a échoué sur un verrou de sandbox avant toute modification et ne figure pas au classement.

Sources officielles : [release RTK](https://github.com/rtk-ai/rtk/releases/tag/v0.50.0), [README RTK](https://github.com/rtk-ai/rtk/blob/develop/README.md), [installation Headroom](https://github.com/headroomlabs-ai/headroom/blob/main/README.md), [API Headroom](https://github.com/headroomlabs-ai/headroom/blob/main/wiki/integration-guide.md).
