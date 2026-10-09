# Hooks natifs

`lm-resizer init --client gemini|copilot|cursor --project-dir .` crée une configuration locale. Un fichier existant est refusé sans `--force`. Aucun agent n'est démarré par cette commande. `all` garde sa portée historique Codex + Claude ; les trois nouveaux clients sont explicites.

Le pré-hook réécrit seulement une commande simple déjà prise en charge, sans redirection ni opérateur de shell. Les autres arguments de l'outil restent intacts. La commande enveloppée enregistre elle-même le tee et les statistiques ; aucun post-hook supplémentaire ne compte deux fois cette exécution.

Les adaptateurs utilisent `BeforeTool` et `hookSpecificOutput.tool_input` pour Gemini ([référence officielle](https://geminicli.com/docs/hooks/reference/)), `preToolUse` et `modifiedArgs` pour Copilot ([référence officielle](https://docs.github.com/en/copilot/reference/hooks-reference)), `preToolUse` et `updated_input` pour Cursor Agent ([référence officielle](https://prod.cursor.com/docs/hooks)). Schémas consultés le 3 octobre 2026. Cursor CLI n'est pas assimilé à Cursor Agent. Une commande reçue en tableau JSON reste un tableau dans `updated_input`/`updatedInput` (`["<lm-resizer>", "exec", "--", …]`, sans shell) ; avant, ses éléments étaient recollés avec des espaces. Le crochet Cursor répond `permission: "ask"` (avant : `allow`, « proceed » pour Cursor) : c'est la seule valeur du schéma qui ne donne pas de feu vert ; la documentation dit `ask` non appliqué pour `preToolUse` et ne dit pas si `updated_input` l'est alors (voir `SECURITY.md`, documentation relue le 9 octobre 2026).

Les tests exécutent le CLI, créent les configurations dans un répertoire temporaire et vérifient les réponses JSON, les arguments conservés, l'idempotence quand le contenu est identique, et le refus d'écraser un fichier divergent sans `--force`. Aucun test de session authentifiée Gemini, Copilot ou Cursor n'est annoncé.

`init` / `init-native-hooks` est idempotent : un second appel avec le même contenu réussit sans `--force` ; un fichier divergent reste refusé. `install-hooks` réutilise la même règle pour les helpers. `uninstall-hooks` retire le bloc marqué, les helpers `.lm-resizer/hooks`, et une config native seulement si elle correspond encore au JSON généré (codex, claude, gemini, copilot, cursor, ou `all` pour codex+claude). Un second `uninstall-hooks` reste sans effet.

