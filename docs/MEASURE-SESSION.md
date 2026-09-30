# Mesurer les jetons d'une session complète

`measure-session` lit des journaux JSONL Claude Code contenant les objets `message.usage` des réponses assistant. Le total vient des compteurs déclarés par le fournisseur. Le texte du journal et le tokenizer local ne servent pas à ce total.

```bash
lm-resizer measure-session session-reference.jsonl --optimized session-optimisee.jsonl --json
```

Sans `--optimized`, la commande affiche le total de la seule session de référence et **aucun pourcentage d'économie**. Les deux captures doivent couvrir la même tâche pour que leur comparaison ait du sens. Elles doivent inclure toutes les réponses du fournisseur ; cette exhaustivité doit être vérifiée au moment de la capture, car un fichier JSONL tronqué peut rester syntaxiquement valide.

Pour chaque `message.id` assistant distinct, le total est `input_tokens + cache_creation_input_tokens + cache_read_input_tokens + output_tokens`, conformément aux [catégories d'usage documentées par Anthropic](https://docs.anthropic.com/en/docs/about-claude/pricing). Les deux compteurs de cache absents valent zéro quand le cache n'est pas utilisé. Plusieurs instantanés du même message ne comptent qu'une fois : l'entrée et le cache doivent être identiques, et le plus grand `output_tokens` est retenu pour un flux cumulatif. Les sommes sont contrôlées contre les dépassements. Une ligne JSON invalide, un identifiant manquant, un usage absent ou un compteur invalide provoque un refus, jamais un résultat partiel. Les messages utilisateur ne portent pas de compteur fournisseur et ne sont pas additionnés séparément : leurs jetons sont inclus dans les compteurs d'entrée des requêtes assistant qui les consomment, avec les prompts système et schémas d'outils.

Le pourcentage A/B vaut `(total_reference - total_optimise) / total_reference × 100`, arrondi à deux décimales. Le JSON donne aussi les deux totaux entiers et leur différence, pour recalculer le chiffre. Il s'agit d'une variation de **jetons déclarés**, pas d'une économie de facture : les tarifs dépendent notamment du modèle et du cache. Le résultat ne prouve pas non plus l'équivalence de qualité ni que le filtrage a causé la différence. Pour une affirmation publique de bénéfice, conserver les deux captures complètes, le modèle, la tâche et un oracle de qualité indépendant.

La commande relie chaque `tool_result.tool_use_id` au `tool_use.id` qui le précède. Elle rejette un résultat sans appel correspondant. Elle rejoue les sorties `Bash` avec le filtre de la commande trouvée et affiche le filtre, l'identifiant et les jetons avant/après selon `o200k_base`. **Ces jetons de rejeu ne sont jamais soustraits du total fournisseur ni utilisés dans le pourcentage A/B.** Les autres outils, images et entrées structurées sont déjà couverts par les compteurs fournisseur ; ils ne sont pas rejoués par le filtre Bash.

## Exemple technique vérifiable

```bash
lm-resizer measure-session fixtures/exec/synthetic_session.jsonl --optimized fixtures/exec/synthetic_session_optimized.jsonl --json
```

La référence contient deux réponses assistant : `100 + 30 + 50 + 20 = 200` et `120 + 0 + 60 + 10 = 190`, soit **390** jetons. La capture optimisée contient `80 + 20 + 40 + 20 = 160` et `90 + 0 + 40 + 10 = 140`, soit **300**. Différence : **90** ; `90 / 390 × 100 = 23,0769… %`, arrondi à **23,08 %**. `TEST TECHNIQUE` et ces usages sont synthétiques : cet exemple valide le calcul, sans mesurer une économie réelle.

Pour une comparaison de compteurs fournisseur déjà regroupés par cas et assortis d'un oracle `quality_pass`, voir [la mesure A/B des sorties](MESURE-AB-SORTIE.md).
