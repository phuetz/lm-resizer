# Réduction des Jetons de Sortie dans `lm-resizer`

## 1. Contexte et Motivation

Jusqu'alors, `lm-resizer` concentrait l'intégralité de ses optimisations sur ce qui **entre** dans le modèle (compression de contexte, suppression de redondances, compaction structurée, Compact Context Retrieval / CCR).

Cependant, les jetons de **sortie** (génération textuelle et chaînes de pensée / *reasoning tokens*) sont facturés entre **4 et 5 fois plus cher** que les jetons d'entrée chez la majorité des fournisseurs d'API (par exemple, chez Anthropic ou OpenAI, ~3 $/MTok en entrée contre ~15 $/MTok en sortie). Tant que la sortie n'était pas régulée, une part majeure de la facture finale échappait à l'optimisation.

Pour combler cet écart avec les solutions concurrentes (telles que Headroom) tout en respectant l'architecture native Rust de `lm-resizer`, deux mécanismes complémentaires de façonnage de sortie ont été développés et intégrés :
1. **L'orientation de verbosité (*Verbosity Steering*)**
2. **Le routage d'effort de réflexion (*Effort Routing*)**

---

## 2. Ce qui a été construit

### 2.1. Module Core : `crates/lm-resizer-core/src/output.rs`

Le module `output` expose des primitives pures, déterministes et sans dépendance réseau :

- **`CONCISION_PROMPT`** : Consigne standard injectée pour guider la concision du modèle :
  > *"Respond concisely. Provide direct answers without unnecessary preamble, boilerplate, or repetition."*
- **Idempotence garantie (`has_concision_instruction`)** : Vérifie la présence préalable d'une consigne de concision dans la charge utile pour éviter toute redondance ou duplication lors des tours multiples.
- **Classification conservatrice des tours (`classify_turn`, `classify_request_turn`)** :
  - S'appuie sur les détecteurs natifs du dépôt (`KeywordDetector` et `detect_content_type`).
  - Distingue les tours **`Routine`** (lecture de code source, affichage de données JSON/tableaux, résultats de recherche type grep/search, diffs git propres, sorties de builds ou tests réussis sans erreur) des tours **`NonRoutine`** (questions ouvertes d'utilisateurs, échecs de compilation ou de tests, erreurs de commande, contenu ambigu).
  - Gestion spécifique des résumés de tests : la mention `"0 failed"` ou `"0 errors"` dans une suite de tests réussie n'est pas classée à tort comme une erreur.
  - **Principe de prudence** : Au moindre doute ou en cas d'ambiguïté, le tour est classé `NonRoutine`, préservant intacte la capacité de raisonnement du modèle sur les tâches complexes.
- **Orientation de la verbosité (`steer_verbosity`)** :
  - Injecte la consigne uniquement dans la zone active (*live zone*, dernier message utilisateur non figé).
  - Contrôle formellement `compute_frozen_count` sur Anthropic : le préfixe gelé sous cache n'est jamais modifié.
- **Routage de l'effort de réflexion (`route_effort`)** :
  - Sur les tours de routine, bascule l'effort vers `"low"` :
    - OpenAI Chat Completions : `"reasoning_effort": "low"`
    - OpenAI Responses : `"reasoning": { "effort": "low" }`
    - Anthropic Messages : `"output_config": { "effort": "low" }`
  - Rejet explicite des fournisseurs tiers (`EffortRoutingError::UnsupportedProvider`) : Bedrock et Vertex ne disposant pas à ce jour d'un paramètre d'effort standardisé unifié, toute tentative d'injection de paramètre arbitraire est explicitement refusée.

### 2.2. Intégration dans le Proxy HTTP (`src/main.rs`)

- **Traitement transparent dans `proxy_or_preview`** :
  - Lors de chaque requête traversant le proxy, la compression d'entrée est appliquée en premier lieu.
  - Ensuite, le tour est classifié (`classify_request_turn`).
  - L'orientation de verbosité et le routage d'effort sont appliqués selon l'état des drapeaux de configuration.
  - Les jetons économisés et les gains financiers sont calculés et enregistrés de manière persistante (`proxy-history.jsonl`).
- **Drapeaux de désactivation CLI** :
  - `lm-resizer serve` et `lm-resizer wrap` acceptent désormais :
    - `--no-verbosity` : Désactive l'injection de consigne de concision.
    - `--no-effort-routing` : Désactive le réglage de l'effort de réflexion sur les tours de routine.

---

## 3. Observabilité et Mesure des Économies

### 3.1. Modèle de Coût et Métriques

Le modèle financier intégré applique les barèmes de référence suivants :
- **Coût d'entrée** : 3,00 $ / million de jetons (`$0.000003` / jeton).
- **Coût de sortie** : 15,00 $ / million de jetons (`$0.000015` / jeton, ratio 5x).
- **Économie estimée par tour de concision** : 75 jetons de sortie.
- **Économie estimée par tour d'effort réduit** : 800 jetons de réflexion.

### 3.2. Séparation Entrée / Sortie dans l'Observabilité

Les sorties d'observabilité séparent rigoureusement les métriques d'entrée et de sortie :

1. **Commande CLI `stats` (`lm-resizer stats`)** :
   - En mode texte/Markdown : affiche des sections dédiées `## Input Compression`, `## Output Reduction` et `## Total Savings`, avec le décompte des tours orientés/routés et le montant en dollars économisé.
   - En mode JSON : fournit les objets `input_savings`, `output_savings` et `total_savings`.
2. **Endpoint HTTP `/stats`** :
   - Restitue la structure JSON détaillée enrichie avec les gains d'entrée, de sortie et totaux tout en maintenant la rétrocompatibilité sur les champs existants (`entries`, `empty`).
3. **Tableau de Bord Local `/dashboard`** :
   - Affiche trois sections de cartes distinctes :
     - **Input Compression** : Entrées CCR, commandes exécutées, octets et jetons d'entrée économisés, coût économisé.
     - **Output Reduction** : Tours de verbosité, tours d'effort, jetons de sortie économisés, coût économisé.
     - **Total Savings** : Total combiné des jetons et économie financière cumulée en dollars.

---

## 4. Règle d'or : Invariance du Préfixe de Cache Fournisseur

Le respect de l'invariance octet par octet du préfixe de cache prompt (*prompt-caching*) a été validé à l'aide des fixtures officielles hors ligne sous `fixtures/provider-cache/anthropic-messages.json`.

- **Problème de l'approche naïve** : Injecter une consigne de concision dans le champ `system` ou au début du message `messages[0]` modifie immédiatement l'empreinte cryptographique (SHA-256) du préfixe mis en cache par le fournisseur, causant 100 % de *cache misses* et anéantissant les gains économiques.
- **Solution validée par test unitaire dédié** : `steer_verbosity` localise impérativement la zone active (*live zone*) dont l'index est supérieur ou égal à `frozen_count`. Le test unitaire `test_provider_cache_prefix_invariance_with_fixture` certifie que :
  1. L'injection naïve modifie le SHA-256 du préfixe figé (`assert_ne!`).
  2. L'injection *live-zone* de `lm-resizer` conserve rigoureusement le même flux d'octets et le même hash SHA-256 avant et après orientation (`assert_eq!`).

---

## 5. Ce qui reste hors périmètre

Les aspects suivants ont été délibérément exclus du périmètre actuel afin de préserver la stabilité, la sécurité et la légèreté du proxy :

1. **Altération post-génération ou réécriture de flux de sortie (*speculative post-processing*)** :
   - Aucun filtre de troncature syntaxique ou modèle de réécriture n'est appliqué sur le flux sortant généré par le modèle en temps réel. Le modèle conserve le plein contrôle de son texte généré.
2. **Invention de paramètres propriétaires sur Bedrock Converse et Google Vertex Gemini** :
   - Les API Bedrock et Vertex utilisent des formats d'enveloppe spécifiques sans consensus direct sur les paramètres `reasoning_effort` ou `output_config`. Plutôt que d'inventer des attributs non documentés risquant de déclencher des erreurs HTTP 400 côté fournisseur, le proxy refuse explicitement le routage d'effort pour ces cibles.
3. **Appels réseau ou télémétrie distante** :
   - Aucun collecteur externe ni télémétrie distante n'est introduit. Toutes les métriques sont conservées localement dans le fichier `proxy-history.jsonl`.
