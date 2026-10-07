# Contre-revues de publication, Cargo et diagnostics — 03/10/2026

Le README français est aligné sur le banc visible. La garde conserve les assertions pytest, messages de panic/unwrap, localisations de pile, erreurs npm et annulations Docker, sans plafond de longueur ni de nombre de diagnostics. La couleur ANSI seule peut changer sans être une perte de texte; les répétitions sont comptées.

Source mesurée : `49e2b7bd2b23908decc10103f5abfdbbf67b7af0`. SHA-256 du CLI : `833b40068fdecb8036a902cf0b60b5879dfe660978e46286b482344f4e17eeff`. Sources propres lors de la compilation. Cargo a annoncé l’exécutable `target/release/lm-resizer`; le constructeur ne suppose plus son emplacement.

## Rejeu du binaire final

| Corpus | Médiane cl100k / o200k | Moyenne cl100k / o200k | Faits exigés visibles | Codes / comptes CLI vérifiés |
|---|---:|---:|---:|---:|
| 30 principaux historiques, 35 contrôles avec les comparaisons | 0,00 % / 0,00 % | 1,49 % / 1,49 % | 160 014 / 160 014 | 35/35 |
| 11 exécutions réelles ajoutées lors de la reprise précédente | 0,00 % / 0,00 % | −4,63 % / −4,85 % | 186 / 186 | 11/11 |
| 41 principaux, 46 contrôles au total | 0,00 % / 0,00 % | −0,15 % / −0,21 % | 160 200 / 160 200 | 46/46 |

Les captures sont celles du [banc visible précédent](../visible/RESULTATS.md), rejouées sans les décoder. Les économies signées et le décompte des faits restent identiques. Tee est vérifié sur 1/1 cas qui en crée un; 45 cas n’en créent pas. RTK n’a pas été relancé ici : ses 43,21 % de médiane o200k concernent le comparatif historique sur 30 commandes, avec ses faits non reconnus publiés à côté. La parité d’économie reste non atteinte.

## Provenance et tests de refus

- `test_build.py` compile réellement un petit programme avec `CARGO_BUILD_TARGET`, puis avec `build.target`, en laissant `STALE-NOT-THE-BUILD` dans le chemin par défaut. Il exécute le binaire indiqué par Cargo et vérifie que le faux ancien fichier reste distinct.
- Revenir au chemin supposé dans une copie du constructeur fait échouer les deux variantes de ce test. Les manifestes anciens sans artefact Cargo et les binaires dont le hash a changé sont refusés.
- Le véritable pilote perf refuse un manifeste au hash erroné avant toute mesure. Son candidat et son manifeste sont obligatoires. L’ancien `--before` reste un chemin choisi explicitement, pas une certification de compilation du binaire avant.
- Les tests Rust du manifeste sont des intégrations ordinaires, exécutées par `cargo test`; les tests Python Cargo et bissection tournent aussi en CI.
- L’ancien binaire `34fe6e49…` de la reprise précédente échoue au nouveau test de compression générique : il perd le message `Option::unwrap()`. La version corrigée passe, ainsi que les contrôles d’assertions, localisations, diagnostics colorés et 350 messages de plus de 400 caractères.
- Les tests du proxy n’exigent plus une réduction destructrice de 400 diagnostics : ils vérifient toutes leurs lignes. Un test CLI Git de 80 commits ferme aussi la réserve de couverture des auteurs.

## Performance du binaire final

Rejeu de sorties épinglées, **hors parcours disque**, deux processus froids par gros volume. Chaque invocation a son propre état SQLite/tee conservé dans le dossier de résultats. Ce sont des seuils absolus, pas une garantie de facteur d’accélération par rapport à une ancienne version. Les fixtures `code_*` passent explicitement par cat.

| Cas | Maximum s | Seuil bloquant |
|---|---:|---|
| ls-real | 0.111 | 2 s |
| ls-5mb | 0.160 | 2 s |
| find-real | 0.154 | 2 s |
| find-5mb | 0.187 | 2 s |
| cat-paths-3mb | 0.113 | aucun; repère 0,3 s indicatif |
| cat-3mb | 0.120 | aucun; repère 0,3 s indicatif |

Longues lignes homogènes, une mesure par taille et mode, vue littérale exigée :

| Octets | tool-output s | exec s |
|---:|---:|---:|
| 250000 | 0.456 | 0.086 |
| 500000 | 0.182 | 0.243 |
| 1000000 | 0.433 | 0.445 |
| 1048576 | 0.528 | 0.499 |
| 20000000 | 13.850 | 13.171 |

Les tests non UTF-8 de capture normale, streaming et nom réel de fichier restent verts. Les comptes de la ligne homogène entière ne sont pas comparés ici au tokenizer Python lent; l’équivalence BPE est testée séparément contre la référence sur les cas du cœur.

## Limites

Pas de nouveau lancement Docker/npm/pytest/tsc dans cette session : leurs captures réelles antérieures sont rejouées. Pas de mesure nouvelle de RTK, Windows/macOS, autres architectures ou fonctionnalités optionnelles. Une liste de signaux diagnostiques n’est pas une preuve universelle sur tous les formats futurs. Les tests de comptes exacts ne certifient pas l’utilisation d’un nombre précis de threads; le banc publie la latence effectivement mesurée. Aucun seuil relatif avant/après ni économie de facture API n’est annoncé.

Validation : `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, tests Python de construction et de bissection, 88 vues adverses littérales, 46 rejeux, quatre mutations de l’oracle visible. Aucune tâche de fond.
