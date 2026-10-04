# Filtres natifs — reprise du 3 octobre 2026

> Reprise suivante : les résultats de cette page sont conservés comme historique. Voir [la correction des patches et son rejeu](patch-revision.md) pour les chiffres actuels, les nouveaux écarts et la preuve de rejet du code précédent.

**Objectif médian dépassé ; parité stricte partielle, 56/61.** Produit autonome : aucun moteur concurrent ni dépendance correspondante. Les filtres sont écrits dans l’architecture LM. La [matrice des 88 commandes](matrix.md) distingue disponibilité, mesure et limites ; elle ne revendique pas 88 parités.

## Reprise de conception

Le [contre-audit de similarité et les corrections](source-independence.md) documentent la suppression des routes mortes, les nouveaux modèles de patch et de regroupement, et la barrière contre les clones normalisés. Le [rejeu après réécriture](independence-family.json.gz) conserve les 61 vues et leurs coûts de la livraison précédente ; `git log -p`, absent de ce corpus, est désormais testé de bout en bout.

## Corpus principal : 61 captures inchangées

| Vue | Médiane économisée | Moyenne économisée |
|---|---:|---:|
| LM avant cette reprise, tee compris | 2,17 % | 25,81 % |
| RTK 0.50.0 | 15,81 % | 32,61 % |
| LM hors tee | 21,74 % | 33,80 % |
| LM tee compris | **21,74 %** | **32,65 %** |
| Headroom 0.39.1, API par défaut | 0,00 % | 1,91 % |

**61/61 bruts exacts ; 61/61 statuts du producteur.** Seulement **52/61 statuts identiques à l’oracle** : neuf routes pipe du concurrent rendent zéro malgré le producteur en échec. LM conserve ce dernier, choix utile pour un agent. Aucun de ces neuf écarts n’est présenté comme parité de statut.

Le banc strict retourne **1** à cause des cinq différences suivantes. Ni normalisation des vues, ni exception automatique pour TSC. Jetons o200k_base, tee inclus dans la métrique finale. Le dénominateur historique `raw_tokens` comprend le séparateur textuel `[stderr]` du corpus lorsqu’il existe ; le tee natif contient les octets réellement produits, sans ce marqueur. Cette convention préserve la comparabilité avec la cible 15,81 %.

| Capture | RTK | LM hors tee | LM tee compris | Explication |
|---|---:|---:|---:|---|
| fastapi-test | 40 | 239 | 251 | 457 erreurs de collecte et noms conservés ; la référence les omet. |
| compare-diff | 2539 | 2525 | 2535 | Même corps ; instruction de rappel externe retirée et remplacée par le tee LM. |
| visible-docker-logs | 44 | 22 | 22 | Échec réel conservé, répétitions exactes ; aucune affirmation artificielle de zéro erreur. |
| fresh-tsc | 437 | 437 | 437 | Même coût ; ordre déterministe des groupes ex æquo, contre ordre variable de la référence. |
| fresh-jest | 33 | 24 | 35 | Résumé natif : nombre, nom, Expected/Received ; la référence affiche une erreur de parseur et son rappel. |

[Résultats et diagnostics d’échecs par outil](results.json.gz), [vues exactes comparées](views.json.gz), [oracle](oracle.json). Le champ `failures` est une sélection lexicale, pas une preuve exhaustive de sémantique. Le tee garde tous les échecs ; les tests vérifient les noms et comptes des formats reconnus. Sorties inconnues conservées littéralement.

## Jalons par famille

| Jalon | Parités strictes | Médiane tee compris | Preuve |
|---|---:|---:|---|
| Départ | 46/61 | 2,17 % | Revue de la livraison précédente |
| Tests | 51/61 | 5,56 % | [tests](tests-family.json.gz) |
| Paquets | 51/61 | 5,56 % | [paquets](packages-family.json.gz) |
| Conteneurs | 53/61 | 5,56 % | [conteneurs](containers-family.json.gz) |
| Diagnostics | 54/61 | 15,81 % | [diagnostics](diagnostics-family.json.gz) |
| Résumé console des tests et finitions | 56/61 | 21,74 % | [tests console](tests-console.json.gz) |

## Ajouts structurels

Ils s’appliquent aux producteurs hors registre ; les commandes déjà couvertes ne sont pas refiltrées. `outline` et `dedup` sont explicites. Ni SearchCompressor ni DiffCompressor importés : les plis de chemins/correspondances gardent toutes les entrées ; le traitement de diff ne supprime aucun hunk.

| Capture / mode | Brut = RTK | LM tee compris | Headroom spécialisé |
|---|---:|---:|---:|
| python-distributions | 2588 | 1088 | 1051 |
| health-log | 10013 | 56 | 119 |
| rust-source | 7380 | 778 | 4195 |
| npm-lock | 20771 | 16897 | 16893 |
| ripgrep-diff (diff) | 227 | 206 | 199 |
| ripgrep-find (paths) | 3695 | 2497 | 2367 |
| fastapi-diff (diff) | 279 | 273 | 266 |
| fastapi-find (paths) | 61723 | 40132 | 39208 |
| TypeScript-diff (diff) | 2144 | 2083 | 2076 |
| TypeScript-find (paths) | 1361049 | 618190 | 631002 |
| TypeScript-grep (search) | 25859 | 21017 | 20243 |
| compare-diff (diff) | 43402 | 42750 | 42743 |
| compare-grep (search) | 2818 | 2119 | 1971 |
| compare-find (paths) | 14203 | 9031 | 8794 |
| fresh-git-diff (diff) | 98 | 90 | 83 |

[Quatre corpus structurés](structured.json) : journal amélioré de 62 à 56 jetons ; JSON et Rust déjà présents, pas présentés comme nouveaux gains. [Replis supplémentaires](reversible.json) : captures réelles rejouées par un producteur non enregistré. `expand` reconstruit chemins, correspondances et runs octet pour octet. Pour le retrait des lignes `index`, l’inverse intégral est le tee, pas `expand` ; la vue l’annonce. Tous les bruts sont vérifiés.

Cas construits, distincts du corpus réel : deux blocs identiques donnent 6 207 → **3 126** jetons (Headroom spécialisé : 3 135), avec inverse exact des messages JSON et tee exact du document. Seuls les blocs `tool` strictement identiques de la fenêtre fournie sont regroupés ; aucune similarité sémantique ni mémoire cachée entre sessions.

[Contour syntaxique contrôlé](outline.json) : Python **275 → 51** (Headroom 97), TypeScript **274 → 126** (Headroom 98) ; RTK read reste à 275/274 sur ces entrées. Code Explorer 0.1.1 fournit les plages et l’empreinte du source ; Python est aussi validé par son AST, Rust par syn. Source invalide Python : 9 → 9. Signatures, fonctions courtes, accolades dans les chaînes et brut vérifiés. `cat` reste littéral. Les autres langages de l’index ne sont pas annoncés comme validés.

[Table JSON mixte](json-encoding.json) : comparaison d’encodages contrôlée ; les cellules CSV encodées en JSON coûtent davantage, donc ce candidat n’est pas intégré. La table typée conserve null, nombres, chaînes et objets imbriqués. Tests de reconstruction ; CSV employé seulement lorsque les cellules sont toutes des chaînes et qu’il gagne.

## Temps et provenance

L’optimisation isolée du runtime conserve **61/61 comptes de vues** : médiane **12,43 → 11,42 ms**, RTK **2,96 ms**, trois répétitions entrelacées sur la même machine. [Échantillons et empreintes](runtime.json.gz). Les commandes courtes utilisent un runtime courant ; les services gardent leur runtime habituel. La [sonde Linux](runtime-observed.json) observe deux fils pendant exec (principal + CLI), avant comme après, sans travailleurs par CPU sur cette route. Le tokeniseur indexé est conservé (initialisation de l’ordre de 10 µs). Les chiffres varient avec la charge de la machine ; aucune promesse de latence absolue.

Comparaison de la reprise précédente, **sept répétitions par cas**, avant/après sur les binaires de début et de fin : **11,64 → 9,07 ms** (−22,05 %), oracle **3,24 ms** ; moyennes **20,41 → 13,75 ms**, oracle **8,41 ms**. [Échantillons précédents](performance-before-independence.json.gz). Les comptes sont inchangés sur **61/61** cas, sans option autorisant des différences. L’ancienne mesure 48,65 → 29,32 ms est [conservée séparément](performance-previous.json.gz) ; elle n’est pas une base temporelle comparable à la nouvelle session.

Après la reprise de conception : **18,39 → 17,54 ms** de médiane (−4,64 %), oracle **7,06 ms** ; moyennes **28,37 → 27,07 ms**, oracle **14,51 ms**. Sept répétitions entrelacées, mêmes captures et machine, tous les comptes inchangés. [Échantillons et empreintes actuels](performance.json.gz). Le binaire avant est exactement celui de la fin précédente ; les 9,07 ms historiques et la cible absolue de 13 ms ne sont pas reproduits dans cette passe. L'initialisation de l'index reste à 20 µs dans la sonde après ; le premier comptage y coûte 2,73 ms. Ce rejeu établit une absence de régression relative, pas une nouvelle optimisation isolée du tokeniseur.

Le [profilage en processus frais](token-profile.json) situe le gain dans le premier comptage ASCII : **4,15 → 1,28 ms** (commande entière **8,69 → 5,80 ms**, neuf répétitions entrelacées). Le motif de découpage est spécialisé aux classes ASCII seulement lorsque tout le texte est ASCII, puis utilise les mêmes rangs BPE. Le Unicode garde son chemin : **9,02 / 9,18 ms** pour la commande témoin. Tests différentiels contre tiktoken-rs, graine aléatoire de régression conservée et contrôle indépendant tiktoken Python. Le cache conserve brut/vue/vue avec rappel sous 16 Mio ; une vue inchangée ne construit plus de candidat de rappel forcément rejeté.

Dans le rejeu principal final : médianes LM **17.63 ms**, RTK **6.27 ms** ; ce rejeu inclut les lancements Headroom et ne remplace pas la mesure isolée.

L’oracle 0.50.0 est construit séparément depuis l’archive épinglée. `build_oracle.py` compare les **582 fichiers** avant et après compilation et écrit lui-même le reçu ; aucune assertion ajoutée manuellement. HOME et XDG sont isolés. Les anciens bancs portent explicitement leur statut historique.

## Reproduction et limites

```sh
python3 bench/real/build_oracle.py
node scripts/build-release-artifact.cjs native
python3 bench/real/check_native_surface.py target/release/lm-resizer
python3 bench/real/parity_rtk.py --binary target/release/lm-resizer \
  --rtk target/rtk-parity/oracle-build/release/rtk \
  --built-oracle target/rtk-parity/oracle-build/receipt.json \
  --headroom-python target/rtk-parity/venv/bin/python \
  --work target/native-check --repetitions 3
```

Les scripts `benchmark_reversible.py`, `benchmark_outline.py` et `benchmark_json_encoding.py` reproduisent les ajouts avec le Python de mesure. Le premier et le second exigent un dossier `--work` neuf. Le contour demande un Code Explorer local, sans appel de modèle.

Barrières : tests workspace, clippy workspace avec avertissements interdits, formatage, installation documentée, surface du binaire et de toutes les aides, dépendances, anonymisation des fichiers publics et des gzip. **1 348 tests réussis, 0 échec, 3 ignorés** ; clippy workspace sans avertissement. Voir [comptes](tests.json) et [surface](surface.json).

Linux x86_64 seulement. Pas de cluster, de déploiement, ni d’agents authentifiés : les hooks Gemini/Copilot/Cursor sont vérifiés sur configuration et JSON documentés. Les 88 outils n’ont pas tous été exécutés en réel ; les formats non reconnus restent littéraux. Aucun résultat de facturation ou de qualité des décisions d’agent. Le chemin normal `exec` capture aussi les producteurs non enregistrés dans un pipe commun ; seuls `--stream` et `--raw-on-failure` gardent la capture séparée annoncée.

## Renforcement de cette reprise

Les sept capacités structurelles et les fonctions prioritaires de la matrice étaient déjà disponibles. Elles sont rejouées, pas comptées une deuxième fois comme nouveautés. La fenêtre de conversation contenant un objet utilisateur `same_as_message` reste maintenant littérale, sans confusion avec le codec. Le test du diff compare la vue complète et couvre l’absence de métadonnée, sans dépendre d’un panic `unwrap`. Les noms `PASS`, `DB_PASS` et toute variante contenant `PASSPHRASE`, notamment `PASSPHRASE_FILE`, sont masqués. La règle par composant PASS masque aussi `PASS_COUNT` : faux positif conservateur documenté et testé. La règle KEY reste volontairement prudente.

[Rejeu après correction structurelle](followup-structural.json.gz) et [rejeu après inspections](followup-inspections.json.gz) : 21,74 %, 56/61 vues, 61/61 bruts et statuts à chaque commit. Les tests CLI ajoutent JSON et récupération exacte, environnement, configuration, arguments du formateur et sortie en échec.

Un `cargo build --release` sans les flags de publication n’est pas la même recette que le script de construction : il peut embarquer les chemins source. Utiliser `node scripts/build-release-artifact.cjs native` pour l’artefact contrôlé ; ses remappages sont testés par `test-release-paths.cjs`. Le SHA de chaque mesure identifie le binaire effectivement exécuté.

La reprise de conception ne compte pas de nouveau gain structurel : les quatre captures spécialisées, les plis, la déduplication et les contours syntaxiques ont été rejoués sur le binaire final. Les coûts publiés ci-dessus restent identiques. Le [rejeu de cette reprise](results.json.gz) garde les cinq écarts stricts et 61/61 statuts du producteur ; la réserve de confidentialité est couverte de bout en bout par `inspection_commands`.

Le [test de grande capture](large-capture.json) exécute un shim Git alternant stdout et stderr, puis relit `tee read` : **11 059 229 octets** identiques, y compris la sentinelle au-delà de 10 Mio, et code producteur 7 conservé. Le contrôle est intégré à la barrière de publication.
