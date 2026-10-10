# Comptage des jetons de texte

Les chiffres CLI sont des mesures du texte UTF-8 avec **tiktoken-rs 0.11.0 / o200k_base**, l'encodage sélectionné par le tokenizer existant pour `gpt-4o`. Le compteur utilise `encode_ordinary` : les chaînes ressemblant à des jetons spéciaux sont traitées comme du texte ordinaire. Aucun téléchargement, appel fournisseur ni changement du tokenizer de compression n'est nécessaire.

## Ce qui est mesuré

`compress`, `exec` et `tool-output` ajoutent à leur JSON les champs suivants :

| Champ | Sens |
| --- | --- |
| `original_tokens` | Compte du texte avant traitement |
| `compressed_tokens` | Compte du texte final rendu, après les portes de conservation des diagnostics et les marqueurs de récupération |
| `tokens_saved` | Différence signée `original_tokens - compressed_tokens` ; négative si le texte final coûte plus de jetons |
| `tokenizer` | `tiktoken-rs/o200k_base` |
| `token_count_method` | `exact` |

`exec` compte le texte capturé par le CLI ; avec `--stream` ou `--raw-on-failure`, ce texte est réassemblé stdout-puis-stderr, y compris son séparateur `[stderr]`. Ce séparateur et l'annotation de capture ne sont pas écrits dans le tee brut. La disposition ne promet pas la chronologie entre les deux flux. Les hooks et le proxy MCP enregistrent les comptes du texte qu'ils traitent dans le même historique. Les champs d'octets et le texte de sortie restent disponibles.

`stats` JSON/Markdown, les statistiques HTTP et le dashboard utilisent les comptes persistés. `tokens_saved`, `original_tokens` et `compressed_tokens` ne portent que sur `measured_commands`. Les regroupements par filtre et commande appliquent la même règle. Les gains négatifs restent négatifs dans les totaux.

Les anciennes entrées ne conservent que les octets. Elles sont classées dans `unmeasured_commands` ; leur estimation `bytes_saved / 4`, par entrée, est conservée dans `estimated_tokens_saved` avec `estimation_method`. Elle est affichée comme estimation historique et n'est jamais ajoutée au total mesuré. Un enregistrement d'un autre encodage est également exclu du total de référence. Aucun historique existant n'est réécrit.

`discover`, `discover-sessions`, `eval` et `learn` disposent des textes originaux : les différences sont comptées pour chaque sortie, puis additionnées (les sorties distinctes ne sont pas tokenisées comme un seul texte). Il s'agit de gains **potentiels du filtre**, pas d'exécutions enregistrées ni du traitement complet `exec`. Le JSON garde `estimated_tokens_saved` comme alias historique de `tokens_saved`, désormais réellement compté ; les champs `tokenizer` et `token_count_method` précisent sa provenance. La limite d'affichage des candidats ne limite pas le total compté.

Les comptes fournisseur dans `proxy_history.provider_reported` restent ceux reçus dans `usage`, accompagnés de leur convention fournisseur : ils ne sont pas calculés par notre tokenizer et ne représentent pas une économie avant/après. Les méthodes internes explicitement nommées `tokens_saved_estimate` et les budgets heuristiques restent des estimations ; ils ne servent pas aux statistiques mesurées. Les JSON d'embedding core/WASM qui n'exposaient que les octets ne revendiquent pas de compte de jetons.

## Reproduire un compte

Capturer une sortie réelle et conserver les octets d'entrée et de sortie :

```bash
cargo test > /tmp/tests.stdout 2> /tmp/tests.stderr
# Pour refaire le filtrage d'un fichier déjà capturé :
lm-resizer tool-output --command 'cargo test' --input /tmp/tests.stdout --json
# Pour mesurer la sortie complète d'une exécution, avec stderr et marqueurs :
lm-resizer exec --json --raw-on-failure -- cargo test
lm-resizer stats --markdown
# Recompter des fichiers archivés sans lire les métadonnées du CLI :
cargo run --quiet --example count_text_tokens -- /tmp/input.txt /tmp/output.txt
```

Soustraire à la main le compte de sortie de celui d'entrée. Ne pas compter uniquement le texte supprimé : les frontières BPE changent. Comparer, si souhaité, avec l'ancienne estimation `max(0, octets_entrée - octets_sortie) / 4` (division entière). Les résultats du banc publiés dans les README utilisaient déjà `o200k_base` ; ils ne sont pas remplacés par des statistiques d'historique.

Cet encodage est une référence reproductible de la famille GPT-4o. Il ne mesure ni les jetons d'un autre tokenizer (Claude, Llama, etc.), ni l'enveloppe de messages API, ni une facture. Les comptes dépendent du texte exact, y compris des marqueurs de récupération.
