# Mesure A/B réelle des jetons de sortie

`docs/JETONS-SORTIE.md` planifie la verbosité et l'effort avec des constantes (75 jetons de concision, 800 jetons de réflexion). Ces constantes ne sont pas une mesure fournisseur.

`examples/compare_provider_usage.rs` compare, hors ligne, des compteurs d'usage déjà capturés. Aucun appel LLM, aucun accès réseau, aucune lecture de longueur de texte. Le modèle de fixture `TEST TECHNIQUE` est un test technique, pas une preuve d'économie fournisseur.

## Commande

Depuis la racine du dépôt :

```bash
cargo run --example compare_provider_usage -- captures.jsonl
```

La réussite écrit un seul JSON sur la sortie standard. Le refus écrit la raison sur la sortie d'erreur et quitte avec le code 1. Le message cite le `case_id` et n'imprime pas le corps des prompts ou des réponses.

Tests de l'exemple (pas la suite du dépôt) :

```bash
cargo test --example compare_provider_usage
```

## Schéma JSONL

Une ligne JSON par variante. Lignes vides ignorées. Champs lus :

| Champ | Type | Règle |
| --- | --- | --- |
| `case_id` | chaîne non vide | Identifie la paire. |
| `variant` | `baseline` ou `optimized` | Exact, sensible à la casse. |
| `model` | chaîne non vide | Identique dans la paire et dans tout le fichier. |
| `usage` | objet | Compteurs bruts, voir ci-dessous. |
| `quality_pass` | booléen | Oracle fourni avec la capture. Obligatoire des deux côtés. |

`usage` accepte l'une des deux formes OpenAI, jamais un mélange contradictoire :

- Chat Completions : `prompt_tokens` et `completion_tokens` ensemble ;
- Responses : `input_tokens` et `output_tokens` ensemble.

Si les deux formes sont présentes et égales, elles sont acceptées. Un compteur est un entier JSON ≥ 0, dans `u64`. La représentation doit être entière : `1` est accepté ; `1.0`, `1.5`, une notation exponentielle, une chaîne, un booléen, `null` ou une valeur négative sont refusés.

Champs ignorés et non recopiés dans le rapport : `prompt`, `response`, `messages`, `output`, `text`, `total_tokens`, et les détails imbriqués (`completion_tokens_details`, `output_tokens_details`). Les jetons de raisonnement ne sont pas ajoutés une seconde fois : seul le compteur de sortie fourni compte.

### Fixture minimale, deux formes d'usage

```jsonl
{"case_id":"c1","variant":"baseline","model":"TEST TECHNIQUE","usage":{"prompt_tokens":200,"completion_tokens":100},"quality_pass":true}
{"case_id":"c1","variant":"optimized","model":"TEST TECHNIQUE","usage":{"prompt_tokens":100,"completion_tokens":50},"quality_pass":true}
{"case_id":"c2","variant":"baseline","model":"TEST TECHNIQUE","usage":{"input_tokens":100,"output_tokens":40},"quality_pass":true}
{"case_id":"c2","variant":"optimized","model":"TEST TECHNIQUE","usage":{"input_tokens":50,"output_tokens":20},"quality_pass":true}
```

### Fixture minimale, sortie baseline à zéro

```jsonl
{"case_id":"zero","variant":"baseline","model":"TEST TECHNIQUE","usage":{"prompt_tokens":40,"completion_tokens":0},"quality_pass":true}
{"case_id":"zero","variant":"optimized","model":"TEST TECHNIQUE","usage":{"prompt_tokens":20,"completion_tokens":5},"quality_pass":true}
```

`output.savings_percent` vaut `null`. Le rapport reste produit. Aucune division par zéro.

### Fixture minimale, qualité dégradée

```jsonl
{"case_id":"drop","variant":"baseline","model":"TEST TECHNIQUE","usage":{"prompt_tokens":100,"completion_tokens":80},"quality_pass":true}
{"case_id":"drop","variant":"optimized","model":"TEST TECHNIQUE","usage":{"prompt_tokens":10,"completion_tokens":1},"quality_pass":false}
```

Refus explicite. Aucun pourcentage d'économie n'est émis, même si les compteurs baissent.

## Critères de refus

Le fichier entier est refusé. Pas de rapport partiel.

- JSON invalide, types invalides, `case_id` ou `model` vide, variante inconnue ;
- compteur négatif, non entier, hors `u64`, paire de clés incomplète, ou les deux dialectes en désaccord ;
- `quality_pass` absent ou non booléen (qualité non comparable) ;
- doublon `(case_id, variant)` ;
- paire absente (baseline sans optimized, ou l'inverse) ;
- modèles différents dans une paire, ou plus d'un modèle dans le fichier ;
- qualité dégradée : `quality_pass` baseline `true` et optimized `false` ;
- somme intermédiaire qui dépasse `u64` (`checked_add`, pas de saturation silencieuse) ;
- fichier sans aucune paire.

Un optimized `false` alors que le baseline est déjà `false` n'est pas une dégradation : les deux oracles sont comparables. Un optimized `true` après un baseline `false` est compté à part (`optimized_recovered`) et ne devient pas une preuve de qualité.

## Rapport

Objet JSON, entrée et sortie séparées, puis total (somme des deux, encore en `checked_add`) :

- `sample_size` : nombre de paires ;
- `model` : l'unique modèle du fichier ;
- `input`, `output`, `total` : chacun `baseline_tokens`, `optimized_tokens`, `absolute_difference` (signé : négatif si l'optimized consomme plus), `savings_percent` ;
- `quality_pass_counts` : `both_pass`, `both_fail`, `optimized_recovered` ;
- `quality_pass_oracle` : ce que le booléen ne prouve pas ;
- `measurement` : origine des compteurs, exclusion de la longueur de texte et des constantes 75/800, statut de `TEST TECHNIQUE`.

`savings_percent` = `(baseline - optimized) / baseline * 100`, arrondi à deux décimales, half-away-from-zero, en arithmétique entière. Un pourcentage négatif est une hausse de consommation, pas une valeur masquée. Si le baseline du bloc vaut 0, `savings_percent` est `null` (cas requis : sortie baseline à zéro ; même garde sur l'entrée et le total).

Les entiers qui ne tiennent pas dans les nombres flottants JSON usuels sont sérialisés en précision décimale exacte.

## Ce que cette mesure n'est pas

`quality_pass` est l'oracle fourni par l'auteur de la capture. Un `true` ne prouve ni l'équivalence sémantique, ni l'exactitude, ni la réussite de la tâche, ni une facture plus basse.

Un fichier dont le modèle est `TEST TECHNIQUE`, ou toute fixture synthétique, exerce l'outil. Il ne mesure pas un fournisseur réel et ne remplace pas les constantes de planification dans le proxy.
