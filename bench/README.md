# Banc comparatif côté à côte

Lancer depuis une version quelconque de LM Resizer :

```bash
./bench/rejouer.sh /chemin/de/sortie/RAPPORT.md
```

Pour tester un autre binaire sans modifier ce checkout, définir
`LM_RESIZER_BIN=/chemin/vers/lm-resizer` avant la commande. Pour tester le code
d'une autre révision, exécuter ce banc dans le checkout de cette révision.

Le script construit le binaire du checkout courant et installe RTK 0.50.0 et
Headroom 0.39.1 avec ONNX Runtime 1.24.4 uniquement dans `_qa/banc/`. Il régénère les 22 fixtures
synthétiques et les oracles de `cases.json`, puis produit `resultats.json` et
un rapport. Les sorties brutes et les journaux locaux restent dans `_qa/banc/`.
Si Codex CLI et son authentification sont disponibles, trois essais de
réparation du même test complètent `resultats_agent.json` et le rapport.

La comparaison est volontairement conservatrice : les économies ne comptent
que lorsque tous les faits exigés sont présents dans la sortie visible. Le
tokenizer commun est `o200k_base`. Les temps incluent le démarrage de chaque
processus, y compris Python pour Headroom ; ils ne représentent pas le débit
d'un proxy persistant. Les commandes sont simulées par des exécutables qui
rejouent les octets de chaque fixture : aucun test, Docker ou service de la
machine n'est modifié par le passage principal.

RTK emprunte la route déclarée par chaque cas : `pipe --filter` pour les
sorties dont le filtre est connu, sous-commandes natives pour les autres
commandes, `json` pour le fichier JSON et `read` pour les fichiers de code.
LM Resizer utilise `exec` pour les commandes et `compress --input` pour les
fichiers. La sortie comptée remplace le chemin absolu du checkout et du HOME,
ainsi que l'horodatage du fichier tee, par des marqueurs constants ; la sortie
brute reste disponible localement avec son empreinte dans les résultats.
L'économie d'une sortie dont l'oracle est incomplet vaut zéro, et une économie
nulle ne donne aucun gagnant. Le script exécute
`_qa/banc/venv/bin/python -m unittest discover -s bench -p test_banc.py`
avant les mesures pour vérifier ces règles et les oracles.

Le corpus couvre tests Cargo, .NET, npm et pytest en succès et en échec, Git,
Docker, PostgreSQL, journaux, JSON, compilation, C#, Rust, Python, TypeScript,
Go, Java et prose. Chaque cas déclare ses faits indispensables dans
`cases.json`. Le cas JSON accepte la représentation en objets ou la table
compacte de SmartCrusher et vérifie la ligne anormale et trois lignes témoins.
Le cas `git_log` exige les 46 sujets et accepte un SHA abrégé unique.

Sources des adaptateurs et versions : [release RTK](https://github.com/rtk-ai/rtk/releases/tag/v0.50.0),
[README RTK et `pipe`](https://github.com/rtk-ai/rtk/blob/develop/README.md),
[installation Headroom](https://github.com/headroomlabs-ai/headroom/blob/main/README.md),
[API Headroom](https://github.com/headroomlabs-ai/headroom/blob/main/wiki/integration-guide.md).
