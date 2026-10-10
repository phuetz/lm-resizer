## Verdict

LIVRÉ

Les trois bloquants A, B et C sont corrigés sur `fix/lmr-026-windows-2026-10-08`, à partir de `ac01374`. Aucun push. La vérification Windows du périmètre passe ; les tests Unix sont fournis sous `cfg(unix)` et restent à rejouer par le pilote. Aucun résultat Linux ou macOS après correction n’est revendiqué.

| Bloquant | Commit | Test et preuve |
| --- | --- | --- |
| A — une image Unix sans shebang était exécutée par `/bin/sh` | `e77b50c` — Préserver le refus Unix des images sans interpréteur | `windows_launch_contract::invalid_executable_is_126_in_every_exec_mode`, sous `cfg(unix)`, renforcé pour détecter toute exécution du texte. Rouge avant établi par la revue jointe ; renforcement et vert après à rejouer sous Unix. Les deux tests de lancement actifs sous Windows passent. |
| B — perte de provenance de la vue, du champ JSON `streams` et des destinations live | `50448d7` — Rétablir la provenance des flux sans polluer leur archive brute | `windows_stream_view_contract` : 2 nouveaux tests rouges sur `ac01374`, verts après ; `windows_tee_contract::stream_without_json_keeps_live_streams_on_their_native_channels` : rouge avant, vert après. Le test du tee dans les quatre combinaisons d’options passe. Les trois régressions `exec_streams` de la revue restent sous `cfg(unix)`, à rejouer. |
| C — lecture du terminal arrêtée par SIGTTIN | `48b1c5a` — Garder les lectures du terminal Unix dans le groupe de premier plan | Nouveau `exec_terminal::inherited_terminal_input_finishes_in_every_exec_mode`, sous `cfg(unix)` : pty avec terminal de contrôle, saisie `hello`, trois modes, délai borné et nettoyage en cas de blocage. Sonde rouge avant établie par la revue ; nouveau test avant/après à rejouer sous Unix. Les 2 tests Windows d’`interruption_contract` passent. |

### Contrat conservé et fichiers modifiés

A et C modifient `src/capture_interrupt.rs`, le câblage de `src/command_capture.rs`, `tests/windows_launch_contract.rs`, `tests/interruption_contract.rs` et ajoutent `tests/exec_terminal.rs`. Sous Unix, `CommandExt::process_group(0)` remplace le callback `pre_exec` ; Rust 1.91 configure ce groupe dans son chemin `posix_spawn` ([source amont](https://github.com/rust-lang/rust/blob/1.91.0/library/std/src/sys/process/unix/unix.rs#L676)). Lorsque stdin est un terminal, aucun groupe séparé n’est créé et le relais cible le seul enfant. Les API console Windows restent sous `cfg(windows)`.

B modifie `src/main.rs`, `src/command_capture.rs`, `tests/exec_streams.rs`, `tests/windows_tee_contract.rs`, le producteur et le harnais de `tests/support/`, et ajoute `tests/windows_stream_view_contract.rs`. La vue historique est rétablie : stdout puis `[stderr]`, annotation si les deux flux sont présents, champ JSON `streams`, stdout et stderr live sur leurs destinations natives. En `--stream --json`, stdout live précède le rapport JSON, comme sur la base.

Le tee conserve les octets sans balise synthétique ni conversion. En capture séparée, les blocs y suivent l’ordre de drainage ; l’ordre d’émission entre flux n’est pas garanti. Cette limite, la frontière de la vue et leur distinction sont précisées dans `README.md`, `README.fr.md`, `CHANGELOG.md` et `docs/TOKEN-STATISTICS.md`. L’assertion Unix qui attendait `[stderr]` dans le tee est corrigée conformément à la mission ; elle exige maintenant cette balise dans la vue et son absence dans le brut. La capture partagée du mode par défaut est conservée. La branche de streaming partagé devenue inutilisée est supprimée. Aucune dépendance ajoutée, aucun autre défaut fonctionnel traité, aucune fusion de la lane Linux.

### Vérification Windows

Rust 1.95.0, cible `x86_64-pc-windows-msvc`, compilation debug.

- `cargo test --workspace --no-fail-fast --locked -- --skip tokenizer::hf_impl::tests::from_pretrained_invalid_repo_returns_hub_error` : **1 396 passés, 0 échec, 3 ignorés, 1 filtré**, sortie **0**. Résultats comptés dans les résumés du journal brut, pas dans la vue réduite.
- `cargo clippy --bin lm-resizer --locked -- -D warnings` : sortie **0**.
- `cargo fmt --all -- --check` et `git diff --check` : sortie **0**.
- Nouveaux tests B posés sur un export exact d’`ac01374`, avec un target distinct : **3 rouges**, sortie **101**. La vérification des seuls octets du tee était déjà verte sur cet arbre ; les assertions finales de provenance sont vérifiées sur le correctif.

Un premier `cargo test --workspace --no-fail-fast --locked`, sans exclusion, a donné **1 394 passés, 3 échecs, 3 ignorés**, sortie **101**. Deux échecs tenaient au dispositif local : les copies de dépendances et de l’ancien arbre sous `.omx` étaient parcourues par le contrôle « Rust uniquement » ; l’alias Windows `python3` était inaccessible. Les copies ont été déplacées sous un répertoire `target`, et un relais Python local a été utilisé. Ces deux tests passent dans le run final, sans modification de leur code.

Le troisième échec était `tokenizer::hf_impl::tests::from_pretrained_invalid_repo_returns_hub_error`, explicitement hors mission : `Cache directory cannot be found`. Ce constat porte sur le profil Windows du bac à sable ; aucune dépendance au réseau n’en est déduite. Ce test et `crates/` restent inchangés.

Cargo a initialement échoué avant compilation avec `Schannel: SEC_E_NO_CREDENTIALS`. Trois archives manquantes ont été récupérées avec validation TLS puis vérifiées contre les SHA-256 de `Cargo.lock`. Le cache Cargo et le relais Python restent locaux sous `.omx/reprise-026/target/` ; aucun profil ni PATH utilisateur réel n’est modifié.

Preuves locales : `.omx/reprise-026/logs/b-avant.log`, `workspace-windows.log`, `workspace-windows-final.log`, `clippy-final.log`, `fmt-final.log`. Les originaux complets restent dans `.omx/reprise-026/outil/tee/`. Ces fichiers de validation ne sont pas suivis par Git.

### Rejeu Unix requis

Le pilote doit exécuter `cargo test --workspace --release --locked`, puis `cargo clippy --bin lm-resizer -- -D warnings`. Le contrôle ciblé est :

```sh
cargo test --locked -p lm-resizer --test exec_streams --test windows_launch_contract --test exec_terminal --test interruption_contract
```

Les quatre rouges propres à la branche signalés dans la revue sont couverts : `raw_failure_keeps_whitespace_and_stderr_only_is_labeled`, `legacy_filter_cannot_discard_the_stderr_boundary`, `non_utf8_bytes_survive_tee_in_captured_and_streamed_execution` et `invalid_executable_is_126_in_every_exec_mode`. Le nouveau test pty couvre le blocage supplémentaire. Les preuves rouges A/C viennent de la revue indépendante ; leur vert après correction reste à mesurer sous Unix.

Risques résiduels : ordre inter-flux limité à l’ordre de drainage dans les modes séparés ; en mode terminal Unix, le relais cible seulement l’enfant immédiat pour préserver le groupe de premier plan ; macOS et Unix non exécutés ici. Les réserves non bloquantes de l’annexe sur la console Windows et la signature réelle ne sont pas élargies en nouvelles corrections.

## Mesure des outils

**9 commandes enveloppées de cette reprise**, sélectionnées dans l’état isolé de LM Resizer. **12 entrées synthétiques** produites par les tests sont exclues. Les commandes supplémentaires de bootstrap et les essais d’agent utilisant un autre état ne sont pas comptés.

| Mesure | Résultat |
| --- | ---: |
| Octets originaux | 322 854 |
| Octets des vues | 170 336 |
| Gain net par soustraction | 152 518 |
| Gain selon `stats`, borné à zéro par commande | 153 204 |
| Jetons économisés estimés par `stats` (octets ÷ 4) | 38 301 |

Il s’agit d’une estimation de jetons, pas d’un comptage exact `o200k_base`. Périmètre : `.omx/reprise-026/mesure/exec-history.jsonl` ; résultat de `lm-resizer stats` : `.omx/reprise-026/logs/mesure-outils.json`. Les journaux bruts ont été relus pour identifier tous les échecs du premier run et compter les résultats du dernier.

Code Explorer : **0 requête**. Une tentative `omx explore` a été refusée par le CLI car cette surface est retirée ; les lectures ont utilisé les outils locaux ordinaires.
