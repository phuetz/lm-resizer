# Mission Windows — LM Resizer 0.2.6

Branche : `fix/lmr-026-windows-2026-10-08`. Départ : `f4d6a8e1793fed5130545677b4154b3ed18037f6`.

Tête finale des correctifs vérifiés : `4f7ae1c2c739e180a1264b8083adaa164815a3b0`. Le commit de ce rapport vient ensuite et ne change aucun code ; son hash figure dans la réponse finale. Les huit commits de correctifs respectent Lore et correspondent chacun à un défaut. Leur arbre final est identique à l’arbre testé avant consolidation.

Binaire Windows x64 construit : `lm-resizer 0.2.6`, SHA-256 `47ba146919b28c7b8547228e3caba9ade41837feff96609b39c29978df9d75ba`. Rust/Cargo 1.95.0, PowerShell 7.6.6 ; les contrats des scripts exécutent Windows PowerShell 5.1.

## Verdict
NON LIVRÉ

Les défauts demandés sont corrigés et leurs **11 tests de régression Windows passent**. La barrière workspace reste rouge sur un test préexistant de `lm-resizer-core`, réservé à la lane Linux : `tokenizer::hf_impl::tests::from_pretrained_invalid_repo_returns_hub_error`. Aucun correctif de ce périmètre n’a été ajouté ici.

## Défaut → commit → test

| Défaut | Commit | Test et preuve |
| --- | --- | --- |
| UTF-16LE avec BOM masque ERROR | `274ba7890da4` | [windows_utf16_contract.rs](tests/windows_utf16_contract.rs) : ligne `ERROR erreur utile été résumé` lisible, tee identique au producteur, BOM et CRLF compris. [Rouge initial](.omx/windows-fix/logs/all-red.log), [vert](.omx/windows-fix/logs/utf16-green.log). |
| Options exec changent les statuts de lancement | `0b9db166293f` | [windows_launch_contract.rs](tests/windows_launch_contract.rs) : commande absente → 127, répertoire non exécutable → 126, diagnostic identique dans les trois modes. Tests Unix de permissions et ENOEXEC ajoutés. [Rouge](.omx/windows-fix/logs/contracts-red.log), [vert](.omx/windows-fix/logs/launch-green.log). |
| Original absent/réassemblé dans raw et stream | `90197db1cfb7` | [windows_tee_contract.rs](tests/windows_tee_contract.rs) : octets mixtes exacts dans standard/raw/stream/raw+stream, aucune balise `[stderr]`, `tee_hint` non nul. JSON valide et flux live texte sur stdout. [Rouge initial](.omx/windows-fix/logs/all-red.log), [régression stdout rouge](.omx/windows-fix/logs/live-stdout-red.log), [vert](.omx/windows-fix/logs/stream-green.log). |
| Interruption perd les octets et peut laisser un enfant | `4f7ae1c2c739` | [interruption_contract.rs](tests/interruption_contract.rs) : vraie console cachée, Ctrl-C normal/raw/stream → **0xC000013A**, vue et tee partiels ; arrêt forcé du parent → tee persistant dans les trois modes. Le nettoyage après timeout ne peut pas fabriquer un succès. Contrat SIGTERM Unix au seul PID parent ajouté. [Rouge](.omx/windows-fix/logs/all-red.log), [vert strict](.omx/windows-fix/logs/interruption-review-green.stdout.log). |
| PATH permanent ignoré si PATH session contient déjà la destination | `e821e761b9c0` | [windows_scripts_contract.rs](tests/windows_scripts_contract.rs) : getter/setter simulés ; le contrôle historique laisse le setter inutilisé. [Rouge comportemental sur f4d6a8e](.omx/windows-fix/logs/path-historical-red.log), [vert](.omx/windows-fix/logs/scripts-final-green.log). |
| Bloc AGENTS.md déplacé par --force | `5aafc0c124ca` | [windows_hooks_contract.rs](tests/windows_hooks_contract.rs) : remplacement à la place originale, préfixe/suffixe identiques, fins de lignes Windows conservées. [Rouge](.omx/windows-fix/logs/all-red.log), [vert](.omx/windows-fix/logs/hooks-green.log). |
| ZIP et sidecar périmés après signature | `cc00b394f813` | [windows_scripts_contract.rs](tests/windows_scripts_contract.rs) exécute le vrai script avec un signataire simulé sans clé. Le script historique modifie le staging mais laisse l’EXE non signé dans le ZIP ; le correctif reconstruit ZIP, sidecar puis SHA256SUMS. [Rouge comportemental sur f4d6a8e](.omx/windows-fix/logs/signature-historical-red.log), [vert](.omx/windows-fix/logs/scripts-final-green.log). |
| Premier exemple echo inexécutable sous Windows | `3fc0f438528f` | Une seule ligne d’exemple Windows ajoutée au README. [windows_quickstart_contract.rs](tests/windows_quickstart_contract.rs) exécute `cmd.exe /d /c echo hello`. [Rouge](.omx/windows-fix/logs/quickstart-red.log), [vert](.omx/windows-fix/logs/quickstart-green.log). |

Les tests de texte, de lancement et de tee sont Rust portables. Les tests qui utilisent les événements console ou PowerShell portent `#[cfg(windows)]` et expliquent cette restriction ; les permissions et signaux Unix portent leur restriction correspondante.

## Barrière et contrôles

| Commande | Passés | Échecs | Ignorés | Statut |
| --- | ---: | ---: | ---: | ---: |
| `cargo test --release --locked --workspace` | **1 283** | **1** | **1** | **101** |
| Même commande avec `--no-fail-fast`, pour terminer tous les targets | **1 394** | **1** | **3** | **101** |

Sorties complètes : [barrière finale](.omx/windows-fix/logs/barrier-final.log), [tous targets](.omx/windows-fix/logs/all-final.log). Les rapports JSON et les originaux binaires sont conservés à côté, dans l’état de tee local. Ces comptes additionnent les résumés Cargo ; le passage strict s’arrête après l’échec du core.

Le test Hugging Face appelle `Api::new()`, puis `Cache::default()` et `dirs::home_dir()`. Sous Windows, cela utilise `SHGetKnownFolderPath(FOLDERID_Profile)` plutôt que HOME/USERPROFILE. Dans ce bac à sable, l’appel renvoie **0x80070002**, et hf-hub panique : **`Cache directory cannot be found`**. [Sonde Windows indépendante](.omx/windows-fix/logs/windows-profile-probe.txt). Le test n’est ni ignoré ni modifié ici.

- `cargo fmt --all -- --check` : succès ; `git diff --check` : succès.
- `cargo clippy --release --locked --bin lm-resizer -- -D warnings` : succès, [sortie](.omx/windows-fix/logs/clippy-bin.log).
- `cargo clippy --release --locked --workspace --all-targets` : succès, avec avertissements préexistants, [sortie](.omx/windows-fix/logs/clippy-workspace.log). Cette compilation vérifie aussi les types du workspace.
- La variante workspace avec `-D warnings` échoue notamment sur `items_after_test_module` dans `src/mcp_proxy.rs`, hors périmètre : [sortie](.omx/windows-fix/logs/clippy.log).
- Revue indépendante du périmètre Windows terminée sans défaut ouvert après correction des courses de signaux et du faux positif possible du harnais Ctrl-C.

Schannel a initialement refusé les téléchargements Cargo. Les archives manquantes ont été téléchargées via HTTPS Node avec validation de certificat et SHA-256 comparé à Cargo.lock : [sommes vérifiées](.omx/windows-fix/logs/dependency-checksums.json). Le cache est isolé sous `.omx/windows-fix/target/cargo`, avec une jonction locale pour conserver les chemins Cargo. Le premier passage avait détecté les fichiers Python de ce cache ; son déplacement sous `target` a résolu cet artefact d’environnement sans changer le contrôle Rust.

## Fichiers et simplifications

Code produit : `src/main.rs`, `src/command_capture.rs`, nouveau `src/capture_interrupt.rs`. Scripts : `install.ps1`, `scripts/sign-windows-release.ps1`. README : uniquement l’exemple Windows autorisé. Harnais : les sept fichiers de contrats ci-dessus, `tests/support/mod.rs` et deux producteurs Rust sous `tests/fixtures/`.

Les lecteurs séparés, le réassemblage stdout-puis-stderr et l’ajout de marqueurs ont été supprimés. La capture partagée sert tous les modes. Aucun ajout de dépendance, aucune modification de Cargo.toml/Cargo.lock, des vues longues, de git log, du crochet Claude ou des autres documents.

## Ouvert et non vérifié

- **Barrière workspace non verte** : intégration d’un correctif du périmètre Cargo/core de la lane Linux, ou environnement Windows capable de fournir le dossier de profil, puis relance obligatoire de la commande exacte.
- **Linux non exécuté** : WSL retourne `Wsl/EnumerateDistros/Service/E_ACCESSDENIED`. Les tests portables de lancement et le contrat SIGTERM/groupe sont prêts, sans preuve d’exécution Linux dans cette lane.
- PATH utilisateur réel et nouveaux terminaux : uniquement simulés, afin de respecter l’interdiction de modifier le profil réel.
- Authenticode, certificat, chaîne de confiance, SmartScreen et ZIP réellement signé : non vérifiés ; signataire simulé seulement. Aucune clé utilisée.
- Images Windows invalides 193/216 : classification 126 ajoutée, mais lancement réel non retenu dans le harnais car un dialogue système peut bloquer un test sans surveillance. Le refus d’exécuter un répertoire est vérifié dans les trois modes.
- `TerminateProcess` et SIGKILL restent non interceptables : pas de vue finale possible, récupération limitée aux octets déjà écrits dans le tee incrémental. L’arrêt forcé représente aussi une expiration externe ; exec n’a pas de timeout interne ajouté.
- Le défaut cosmétique des messages d’idempotence, présent dans l’annexe mais non demandé, reste inchangé.

HOME, USERPROFILE, CODEX_HOME, AppData et temporaires des essais sont isolés dans `.omx`. Aucun push, publication, certificat réel ni modification du PATH utilisateur réel. Aucune exécution ni nouvel artefact d’un outil concurrent de compression hors `bench/`.

## Mesure des outils

**18 commandes enveloppées finalisées**, mesurées avec `tiktoken-rs/o200k_base`, comptage exact. La phase de bootstrap et les tests autonomes exécutés directement ne sont pas comptés comme commandes enveloppées.

| Mesure | Résultat |
| --- | ---: |
| Octets originaux | 541 313 |
| Octets des vues | 535 067 |
| Gain octets net par soustraction | 6 246 |
| Gain octets selon stats, borné à zéro par commande | 6 402 |
| Tokens originaux | 135 091 |
| Tokens des vues | 133 178 |
| Tokens économisés nets | 1 913 |

Preuves : [stats des commandes réellement enveloppées](.omx/windows-fix/logs/wrapper-measurement.json), [périmètre de sélection](.omx/windows-fix/logs/measurement-scope.json). Le journal initial était partagé avec les tests synthétiques : 15 entrées synthétiques valides et 3 lignes non JSON ont été écartées. Les stats ci-dessus sont recalculées par `lm-resizer stats --json` sur une copie contenant uniquement les enregistrements des vraies commandes enveloppées. Les vues brutes sur échec dominent ces validations ; ces chiffres ne constituent ni un benchmark général ni une économie financière.
