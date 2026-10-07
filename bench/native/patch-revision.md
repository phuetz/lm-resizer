# Patches réversibles — correction après contre-revue

La contre-revue a raison sur les deux défauts : renvoyer le brut de `git log -p` ne corrigeait pas sa compression ; le premier `patch_view` conservait une politique de troncature trop proche de l’oracle. Cette version retire le modèle de fichiers/hunks et tous ses budgets. Elle n’en renomme pas les étapes.

## Conception et reconstruction

`Patch v1` factorise uniquement quatre en-têtes textuels concordants et les runs de lignes strictement identiques. Aucun numéro de plage n’est interprété et aucune ligne de changement, de contexte ou de commit n’est supprimée. `File` garde le chemin JSON et la ligne index ; `FileT` conserve aussi la tabulation terminale émise par Git pour les noms avec espaces. `Repeat N` signifie N copies supplémentaires de la ligne précédente. Les directives présentes dans le texte sont échappées ; `End 0/1` conserve l’absence/la présence du dernier LF. CRLF reste intact. `expand` reconstruit sans consulter le tee et refuse les vues incomplètes et les expansions démesurées (512 Mio maximum). Le tee reste intégral même au-delà de cette limite du décodeur.

Le candidat n’est utilisé que s’il réduit octets ET jetons. Un texte non compressible reste littéral ; ce n’est plus une exemption systématique pour les historiques avec patches. Les statuts Git propres longs sont raccourcis séparément en `branche = suivi; clean` (égalité de révision), ou `branche; clean` sans suivi. Les états ahead/dirty restent conservés.

## Mesures finales

[61 captures](patch-results.json.gz), o200k_base, trois répétitions (64 pour TSC), même archive et mêmes entrées. Le séparateur historique `[stderr]` du dénominateur reste celui du banc précédent.

| Outil/révision | Médiane, tee compris | Moyenne |
|---|---:|---:|
| LM avant (`1d9b48d`) | 21,74 % | 32,65 % |
| LM après | **21,74 %** | **31,58 %** |
| RTK 0.50.0 | 15,81 % | 32,61 % |
| Headroom 0.39.1, mode général | 0,00 % | 1,91 % |

**49/61 vues strictement identiques**, contre 56 avant. **61/61 bruts exacts, 61/61 codes du producteur**, mais seulement **52/61 codes égaux à l’oracle** : neuf sorties en échec du corpus sont conservées par LM alors que l’oracle retourne zéro. La baisse de moyenne est assumée : le gros diff n’est plus tronqué. Les statuts propres compensent la perte de médiane des patches ; aucun cas ni dénominateur n’a été remplacé.

Le banc strict termine avec le code **1**, à cause des écarts ci-dessous : il n’est pas annoncé vert. Coûts en jetons, LM tee inclus ; aucun écart de tee ou de code producteur.

| Cas différent | RTK | LM | Raison |
|---|---:|---:|---|
| ripgrep-diff | 155 | 181 | Patch réversible sans troncature |
| ripgrep-status | 22 | 17 | Statut propre compact, branche et suivi conservés |
| fastapi-diff | 237 | 253 | Patch réversible sans troncature |
| fastapi-status | 22 | 17 | Statut propre compact, branche et suivi conservés |
| fastapi-test | 40 | 251 | Compteurs conservés par la vue native (écart antérieur) |
| TypeScript-diff | 1805 | 2096 | Patch réversible sans troncature |
| TypeScript-status | 22 | 16 | Statut propre compact, branche et suivi conservés |
| compare-diff | 2539 | 42386 | Patch réversible sans troncature |
| visible-docker-logs | 44 | 22 | Vue native plus courte (écart antérieur) |
| fresh-git-diff | 69 | 87 | Patch réversible sans troncature |
| fresh-tsc | 437 | 437 | Ordre non déterministe de l’oracle (écart antérieur) |
| fresh-jest | 33 | 35 | Présentation native du résumé (écart antérieur) |

[Historiques Git réels générés](patch-log.json) : répétitions **6 642 → 266** jetons ; lignes distinctes **7 605 → 7 586**, contre zéro gain avant dans les deux cas. Deux commits par dépôt, sorties réellement produites par Git ; pas des captures inventées par un shim. Chaque cas vérifie `expand` et `tee read` octet pour octet. Ces deux cas contrôlés ne sont pas ajoutés aux 61 du corpus.

[Mesure temporelle](patch-performance.json.gz) : sept répétitions entrelacées avant/après/oracle, même machine ; médianes **13,16 → 12,30 ms**, oracle **5,70 ms** ; moyennes **18,92 → 18,81 ms**, oracle **11,52 ms**. Les vues changent : `--allow-view-changes` est explicite. Ce n’est pas une optimisation isolée du démarrage, ni une promesse de latence absolue.

## Preuve de rejet de la paraphrase

Le contrôle conserve ses fenêtres lexicales de 32 jetons et son seuil de rejet de 72. Il ajoute une signature de politique répartie entre fonctions : au moins deux limites numériques communes réellement employées pour sélectionner (comparaisons, take/skip/min/max, résolution des affectations littérales), omissions à deux niveaux (local et global), et au moins un message d’omission commun. Pas d’exception par nom de fichier.

[Avant](patch-similarity-before.json.gz) : snapshot exact de `src/` au commit `1d9b48d`, **code 1**, une alerte sur `patch_view.rs` contre `git_cmd.rs` : limites **3 et 100**, omissions locales/globales et message global commun. Le contrôle lexical seul trouvait zéro violation. [Après](patch-similarity-after.json.gz) : **code 0**, zéro violation lexicale ou de politique, 84 rapprochements courts conservés pour audit. Les fichiers locaux et l’archive sont identifiés par SHA-256.

Six tests couvrent notamment les helpers renommés/réordonnés, la conservation de la détection après un module de tests, les exclusions, les limites différentes et le refus d’une archive altérée. La barrière de publication appelle ce contrôle. Il s’agit d’une alarme ciblée, pas d’un détecteur général de toute équivalence sémantique ni d’une preuve juridique d’originalité.

## Validation et reproduction

`cargo test --workspace --offline` : 1 348 réussis, zéro échec, trois ignorés ; `cargo clippy --workspace --offline -- -D warnings` et `cargo fmt --check` passent. [Surface](patch-surface.json) : strings, dépendances et 60 aides sans nom concurrent. [Grande capture](patch-large-capture.json) : 11 059 229 octets entrelacés exacts, sentinelle au-delà de 10 Mio, statut 7. Les tests d’installation documentée et des flags de publication passent.

```sh
node scripts/build-release-artifact.cjs native
python3 bench/real/test_source_similarity.py
python3 bench/real/check_source_similarity.py
# Préserver le binaire avant dans target/patch-revision/lm-before pour cette comparaison.
target/rtk-parity/venv/bin/python bench/real/benchmark_patch.py \
  --binary target/release/lm-resizer --before target/patch-revision/lm-before \
  --output target/patch-check.json
```

Pour les 61 cas, suivre la commande du [banc natif](README.md) avec un répertoire de travail neuf. L’oracle reste construit séparément depuis l’archive vérifiée. Aucun ajout Headroom ni aucune modification du tokeniseur dans cette correction ; les résultats spécialisés antérieurs restent historiques. Pas de validation Windows/macOS ni de preuve de toutes les variantes possibles du format Git. L’historique contenant les anciennes sources n’a pas été réécrit.
