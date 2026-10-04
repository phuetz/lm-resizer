# Matrice des 88 commandes de comparaison

Inventaire fonctionnel, pas 88 promesses de parité. Le rejeu strict contient 61 captures seulement. Une exécution générique conserve le statut et le brut mais ne démontre pas un résumé spécialisé. Aucun moteur concurrent dans le produit.

| Référence | Entrée LM | État et preuve / limite |
|---|---|---|
| ls | `exec -- ls` | Rendu natif ; preuves des familles tests/packages/containers/diagnostics et banc 61. Tous les sous-commandes/formats ne sont pas couverts. |
| tree | `exec -- tree` | Rendu natif ; preuves des familles tests/packages/containers/diagnostics et banc 61. Tous les sous-commandes/formats ne sont pas couverts. |
| read | `read ; outline explicite séparé` | Fonction LM disponible, contrat distinct de la référence. |
| smart | `smart (sémantique LM existante)` | Fonction LM disponible, contrat distinct de la référence. |
| git | `exec -- git` | Rendu natif ; preuves des familles tests/packages/containers/diagnostics et banc 61. Tous les sous-commandes/formats ne sont pas couverts. |
| gh | `exec -- gh` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| glab | `exec -- glab` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| aws | `exec -- aws` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| psql | `exec -- psql` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| pnpm | `exec -- pnpm` | Rendu natif, cas unitaires ; pas de validation exhaustive avec outil réel. Voir limites du rapport. |
| err | `err` | Ajout natif ; unités inspection_views et/ou tests inspection_commands ; --help vérifié. |
| test | `test` | Ajout natif ; unités inspection_views et/ou tests inspection_commands ; --help vérifié. |
| json | `json` | Ajout natif ; unités inspection_views et/ou tests inspection_commands ; --help vérifié. |
| deps | `deps` | Ajout natif ; unités inspection_views et/ou tests inspection_commands ; --help vérifié. |
| env | `env` | Ajout natif ; unités inspection_views et/ou tests inspection_commands ; --help vérifié. |
| find | `exec -- find` | Rendu natif ; preuves des familles tests/packages/containers/diagnostics et banc 61. Tous les sous-commandes/formats ne sont pas couverts. |
| diff | `exec -- diff` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| log | `pipe -f log` | Fonction LM disponible, contrat distinct de la référence. |
| dotnet | `exec -- dotnet` | Résumé natif des tests réussis cohérents ; diagnostics inconnus conservés. Cas dotnet_ok : 17 jetons, tee exact (windows-release/four-final.json). TRX/binlog/JSON préexistants. |
| docker | `exec -- docker` | Rendu natif ; preuves des familles tests/packages/containers/diagnostics et banc 61. Tous les sous-commandes/formats ne sont pas couverts. |
| kubectl | `exec -- kubectl` | Rendu natif, cas unitaires ; pas de validation exhaustive avec outil réel. Voir limites du rapport. |
| oc | `exec -- oc` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| summary | `summary` | Ajout natif ; unités inspection_views et/ou tests inspection_commands ; --help vérifié. |
| grep | `exec -- grep` | Rendu natif ; preuves des familles tests/packages/containers/diagnostics et banc 61. Tous les sous-commandes/formats ne sont pas couverts. |
| rg | `exec -- rg` | Rendu natif ; preuves des familles tests/packages/containers/diagnostics et banc 61. Tous les sous-commandes/formats ne sont pas couverts. |
| ast-grep | `exec -- ast-grep` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| init | `init / init-native-hooks` | Claude/Codex existants ; Gemini/Copilot/Cursor ajoutés et testés en JSON/configuration. Aucun agent authentifié lancé. |
| wget | `exec -- wget` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| wc | `exec -- wc` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| gain | `stats / gain, chaque exec enregistré` | Fonction LM disponible, contrat distinct de la référence. |
| cc-economics | `—` | Exclu selon consigne : aucune télémétrie ; analyse de sessions/coûts non reprise. |
| config | `config` | Ajout natif ; unités inspection_views et/ou tests inspection_commands ; --help vérifié. |
| jest | `exec -- jest` | Rendu natif ; preuves des familles tests/packages/containers/diagnostics et banc 61. Tous les sous-commandes/formats ne sont pas couverts. |
| vitest | `exec -- vitest` | Rendu natif ; preuves des familles tests/packages/containers/diagnostics et banc 61. Tous les sous-commandes/formats ne sont pas couverts. |
| ctest | `exec -- ctest` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| prisma | `exec -- prisma` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| tsc | `exec -- tsc` | Rendu natif ; preuves des familles tests/packages/containers/diagnostics et banc 61. Tous les sous-commandes/formats ne sont pas couverts. |
| next | `exec -- next` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| lint | `exec -- eslint` | Rendu natif ; preuves des familles tests/packages/containers/diagnostics et banc 61. Tous les sous-commandes/formats ne sont pas couverts. |
| prettier | `exec -- prettier` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| format | `format` | Ajout natif ; unités inspection_views et/ou tests inspection_commands ; --help vérifié. |
| playwright | `exec -- playwright` | Analyse structurée LM préexistante (TRX/binlog/JSON), tests workspace ; essais réels non rejoués ici. |
| cargo | `exec -- cargo` | Rendu natif ; preuves des familles tests/packages/containers/diagnostics et banc 61. Tous les sous-commandes/formats ne sont pas couverts. |
| npm | `exec -- npm` | Rendu natif ; preuves des familles tests/packages/containers/diagnostics et banc 61. Tous les sous-commandes/formats ne sont pas couverts. |
| npx | `exec -- npx` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| bun | `exec -- bun` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| bunx | `exec -- bunx` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| curl | `exec -- curl` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| discover | `discover, analyse LM propre` | Fonction LM disponible, contrat distinct de la référence. |
| session | `—` | Exclu selon consigne : aucune télémétrie ; analyse de sessions/coûts non reprise. |
| telemetry | `—` | Exclu selon consigne : aucune télémétrie ; analyse de sessions/coûts non reprise. |
| learn | `learn, recommandations LM propres` | Fonction LM disponible, contrat distinct de la référence. |
| run | `run` | Ajout natif ; unités inspection_views et/ou tests inspection_commands ; --help vérifié. |
| proxy | `proxy` | Ajout natif ; unités inspection_views et/ou tests inspection_commands ; --help vérifié. |
| recall | `recall --list ; tee list/read` | Fonction LM disponible, contrat distinct de la référence. |
| pipe | `pipe -f` | Fonction LM disponible, contrat distinct de la référence. |
| trust | `trust-filters` | Fonction LM disponible, contrat distinct de la référence. |
| untrust | `untrust-filters` | Fonction LM disponible, contrat distinct de la référence. |
| verify | `verify-filters` | Fonction LM disponible, contrat distinct de la référence. |
| ruff | `exec -- ruff` | Rendu natif, cas unitaires ; pas de validation exhaustive avec outil réel. Voir limites du rapport. |
| sqlfluff | `exec -- sqlfluff` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| pytest | `exec -- pytest` | Rendu natif ; preuves des familles tests/packages/containers/diagnostics et banc 61. Tous les sous-commandes/formats ne sont pas couverts. |
| mypy | `exec -- mypy` | Rendu natif, cas unitaires ; pas de validation exhaustive avec outil réel. Voir limites du rapport. |
| php | `exec -- php` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| phpunit | `exec -- phpunit` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| phpstan | `exec -- phpstan` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| pest | `exec -- pest` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| paratest | `exec -- paratest` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| ecs | `exec -- ecs` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| pint | `exec -- pint` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| phpt | `exec -- phpt` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| rake | `exec -- rake` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| rubocop | `exec -- rubocop` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| rspec | `exec -- rspec` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| pip | `exec -- pip` | Rendu natif, cas unitaires ; pas de validation exhaustive avec outil réel. Voir limites du rapport. |
| uv | `exec -- uv` | Rendu natif, cas unitaires ; pas de validation exhaustive avec outil réel. Voir limites du rapport. |
| deno | `exec -- deno` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| go | `exec -- go` | Rendu natif, cas unitaires ; pas de validation exhaustive avec outil réel. Voir limites du rapport. |
| sbt | `exec -- sbt` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| gt | `exec -- gt` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| golangci-lint | `exec -- golangci-lint` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| gradlew | `exec -- gradlew` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| mvn | `exec -- mvn` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| mvnd | `exec -- mvnd` | Capture native et filtres LM existants / repli littéral. Aucun gain spécialisé ni parité revendiqués sur cette reprise. |
| hook-audit | `hook-audit` | Ajout natif ; tests inspection_commands et unités inspection_views. hook-audit : compteurs locaux sans arguments. |
| rewrite | `rewrite, dont head simple → read` | Fonction LM disponible, contrat distinct de la référence. |
| hook | `hook --client --event` | Claude/Codex existants ; Gemini/Copilot/Cursor ajoutés et testés en JSON/configuration. Aucun agent authentifié lancé. |
| help | `--help` | Fonction LM disponible, contrat distinct de la référence. |

Les fonctions de confiance concernent les filtres LM. `read` et `exec -- cat` restent littéraux ; `outline` est explicite. Les outils absents ne sont ni téléchargés ni exécutés via un remplaçant implicite. `sg` ne doit pas être assimilé à `ast-grep` lorsqu’il désigne un autre programme.
