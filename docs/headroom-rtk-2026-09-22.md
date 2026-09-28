# Écarts Headroom / RTK — intégration et preuves du 22 septembre 2026

Cette branche rassemble des travaux auparavant séparés. Elle ne constitue pas une publication, ni une preuve de parité totale avec les deux outils.

| Sujet | État dans cette branche | Preuve / limite |
|---|---|---|
| Consigne de concision et routage de l'effort | Intégré depuis PR #15, tête `089151a` | Configuration explicite ; ne pas transformer les constantes de planification en économies mesurées. |
| Compression syntaxique / conseiller de plages | Intégré depuis PR #16, tête `7e127bc` | Rust, JavaScript/TypeScript et Python ; analyse approximative, pas une parité avec tous les langages. |
| Proxy MCP transparent | Intégré depuis PR #17, tête `f25a6e0` | Liste d'autorisation des réponses `tools/call`, préservation des autres messages et récupération CCR. |
| Refus d'une sortie plus grosse | Intégré depuis PR #18, tête `a75634e` | Corpus figé, filtres et sortie finale ; marqueurs compris. |
| Mesure A/B des jetons fournisseur | Nouvel exemple `compare_provider_usage` | Entrée/sortie séparées, usage fourni, refus des paires invalides ou à qualité dégradée. Aucun appel réseau. |
| Économie fournisseur réelle | **Non mesurée ici** | Nécessite des captures baseline/optimized comparables et un oracle de qualité adapté ; les fixtures sont synthétiques. |
| Couverture supplémentaire de commandes | **À mesurer et compléter** | Le nombre de commandes n'est pas une preuve du volume économisé. Prioriser les commandes réelles encore routées vers le filtre générique. |
| Langages supplémentaires | **Reste à faire selon corpus** | Ne pas présenter l'analyse approximative actuelle comme un parseur exhaustif. |

## Vérifications réellement exécutées

- Ensemble des quatre lots : `cargo test --workspace --no-default-features` — **1 103 tests passés, 0 échec, 3 ignorés** (dont deux exemples de documentation).
- `cargo fmt --check` : vert.
- `cargo check --release --examples` : vert sur l'intégration.
- `scripts/check-release.sh` : vert, incluant tests release, smoke proxy, vérification WASM, simulation de publication, preuves et checksums. **Aucune publication réelle**.
- Nouvel exemple : `cargo test --example compare_provider_usage` — **7 tests passés** ; cas décimal `1.0` ajouté au groupe des usages invalides lors de la revue.
- Exécution du nouvel exemple sur une fixture synthétique : rapport JSON parsé ; compteurs 100/40 contre 80/20. Ces nombres sont des données de test, pas une économie mesurée chez un fournisseur.

## Correction d'intégration

La PR #17 rend `ExecReport` accessible au module proxy ; la PR #18 lui ajoute `filter_not_smaller`. La fusion de ces deux changements exige de conserver la visibilité et d'initialiser le nouveau champ dans le proxy. Celui-ci ne journalise ici que les sorties effectivement plus petites, donc le champ vaut `false`. Le premier build groupé a détecté l'initialiseur incomplet ; le build corrigé et les tests passent.

## Origine des travaux

Mistral Vibe : intégration locale des PR #15 à #17. Codex : intégration de #18, correction d'interface et vérification de l'ensemble. Grok : exemple A/B et documentation, relus et testés par Codex. Les PR d'origine restent ouvertes ; leur fusion distante n'a pas été effectuée par cette campagne.

## Reproduire la mesure

Voir [MESURE-AB-SORTIE.md](MESURE-AB-SORTIE.md). La conservation d'un indicateur `quality_pass` dans une capture ne prouve pas à elle seule la fidélité : documenter le critère employé, les échecs, le modèle et le corpus. Les compteurs d'entrée ne sont pas une estimation de la facture, notamment avec cache fournisseur.

Documentation concurrente consultée le 22/09/2026 : [Headroom](https://github.com/headroomlabs-ai/headroom/blob/main/README.md), [RTK](https://github.com/rtk-ai/rtk/blob/develop/docs/guide/index.md). Leurs taux annoncés n'ont pas été reproduits ici.
