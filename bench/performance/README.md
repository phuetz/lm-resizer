# Coût des processus courts

Mesure sur la même machine, 61 captures fixes, sept répétitions par outil et par
cas, ordre avant/après/RTK tournant. Chaque mesure crée un nouveau processus ;
les caches du système ne sont pas vidés. Le fichier `results.json` contient les
échantillons, les empreintes des trois binaires et le profil du tokeniseur.

| Temps (ms) | LM avant | LM après | RTK officiel |
|---|---:|---:|---:|
| Médiane des médianes par cas | 59,54 | 24,92 | 3,72 |
| Moyenne des médianes par cas | 69,23 | 31,16 | 9,28 |

La médiane baisse de **58,14 %**. Le profil attribue 38,05 ms à la reconstruction
du vocabulaire avant, contre 0,008 ms après. Le premier comptage, compilation de
la regex comprise, passe de 4,06 à 2,79 ms sur le petit cas de diagnostic. Les
comptes exacts avant/après sont identiques dans les sept répétitions des 61 cas.

Le chargement de 199 998 entrées allouait et remplissait une table de hachage à
chaque invocation. L'index est maintenant construit avec le vocabulaire pendant
la compilation et consulté directement dans les données du binaire. Aucune
approximation, aucun service résident ni cache partagé entre processus n'est
utilisé. Les tests vérifient toutes les entrées du vocabulaire et comparent les
fusions courtes et longues à tiktoken-rs.

Il reste un écart : LM démarre son CLI, conserve le brut et compte réellement les
jetons ; les runners directs utilisent aussi un processus isolé et le traçage
nécessaire à l'ordre des écritures. RTK ne fournit pas ce même contrat de capture.
Ces chiffres ne sont pas directement comparables aux 144,89/19,51 ms de la revue,
mesurés à un autre moment sous une charge différente. Le protocole ci-dessus
compare les deux versions pendant la même exécution.

```sh
python3 bench/real/benchmark_startup.py \
  --before /path/to/lm-before --after target/release/lm-resizer \
  --rtk target/rtk-parity/bin/rtk --work target/new-performance --repetitions 7
```

Le binaire avant correspond à l'objet publication, avant l'optimisation du
compteur ; les SHA-256 dans le résultat identifient exactement les exécutables.
