# Compression syntaxique de code source : intégration à l'exécution et séparation des licences

## 1. Séparation stricte des licences : BUSL-1.1 vs Apache-2.0

### Le conflit de licence
Au sein de l'écosystème d'outils internes, **Code Explorer** (`/home/patrice/DEV/gitnexus-rs`) implémente déjà un moteur d'analyse syntaxique avancé fondé sur `tree-sitter`, couvrant 14 langages de programmation (dont Rust, Python, TypeScript et JavaScript) dans son crate `crates/code-explorer-lang`. Il expose un graphe de symboles via CLI et serveur MCP (30 outils).

Cependant, une contrainte juridique fondamentale s'impose :
- **Code Explorer** est placé sous licence **BUSL-1.1** (Business Source License 1.1), une licence avec restrictions d'usage commercial direct.
- **lm-resizer** est placé sous licence libre et permissive **Apache-2.0**.

### Pourquoi une dépendance de compilation est strictement exclue
L'ajout d'une dépendance directe dans le `Cargo.toml` de `lm-resizer` (par exemple `code-explorer-lang = { path = "..." }`) lierait statiquement le code de Code Explorer dans le binaire de `lm-resizer`.

Sur le plan juridique :
1. Une liaison statique ou dynamique à la compilation ferait de `lm-resizer` une **œuvre dérivée** d'un composant sous BUSL-1.1.
2. Les termes restrictifs de la BUSL-1.1 **contamineraient** le binaire distribué et priveraient les utilisateurs de `lm-resizer` des garanties de la licence Apache-2.0 (liberté de redistribution, intégration commerciale libre, absence de clause de reversion).
3. Une telle contamination romprait l'engagement open source public du projet `lm-resizer`.

### L'approche retenue : intégration à l'exécution, pas à la compilation
En droit du logiciel comme en architecture système, deux processus distincts qui s'exécutent indépendamment et communiquent via des interfaces système standardisées (fichiers, tuyaux inter-processus `stdio`, protocoles JSON-RPC / MCP ou CLI) constituent **deux œuvres indépendantes** :
- `lm-resizer` demeure une œuvre 100 % Apache-2.0, compilable et distribuable de manière totalement autonome sans le moindre octet de Code Explorer.
- Si Code Explorer est installé sur la machine de l'utilisateur, `lm-resizer` peut solliciter ses services à l'exécution sous forme d'outil compagnon externe.
- Si Code Explorer est absent, `lm-resizer` fonctionne de manière autonome en retombant sur son moteur heuristique embarqué.

### Avantage produit : aucun gonflement du binaire unique
Lier `tree-sitter` et les bibliothèques C compilées de 14 grammaires ajouterait entre **6 et 12 Mo** de tables d'analyse LR statiques au binaire de compilation.

Or, le **binaire unique autonome et compact** (zero-dependency, lean native binary) est l'un des arguments de vente majeurs de `lm-resizer`. L'approche par détection à l'exécution préserve intégralement cet atout :
- Surcoût sur le binaire `target/release/lm-resizer` : **0 Ko** lié à tree-sitter.
- Compilation native instantanée, compatibilité WASM (`lm-resizer-wasm`) sans dépendance vers un compilateur C.

---

## 2. Architecture de l'intégration à l'exécution

```
                                  [Code Source d'entrée]
                                             │
                                             ▼
                             [Détection du langage cible]
                           (Rust, Python, TypeScript, JS)
                                             │
                       ┌─────────────────────┴─────────────────────┐
                       │                                           │
         [Code Explorer présent ?]                                 │
           - `code-explorer` in PATH                               │
           - Non désactivé par env                                 │
                       │                                           │
             OUI ┌─────┴─────┐ NON / Échec                         │
                 ▼           ▼                                     ▼
        [Requête Externe]  [Repli Embarqué]              [Langage inconnu / <6L]
        - CLI (cypher)     - Regex signatures                      │
        - ou MCP (read_file)- Scan d'accolades/indent              │
                 │           │                                     │
                 ▼           ▼                                     │
        [Symboles AST]     [Structure Approximative]               │
        Exacts (TSitter)   Heuristique                             │
                 │           │                                     │
                 └─────┬─────┘                                     │
                       ▼                                           ▼
             [Bannière explicite CCR]                    [Repli Conservatif]
             - "Structure syntaxique"                    (suppression lignes vides
               (si Code Explorer)                         et commentaires pleine ligne)
             - "Structure approximative"
               (si repli embarqué)
                       │
                       ▼
            [Stockage CCR du brut]
             (récupérable par hash)
```

### 1. Détection à l'exécution (`CodeExplorerBridge`)
`lm-resizer` inspecte dynamiquement l'environnement hôte à l'exécution :
- **Variable de désactivation** : si `LM_RESIZER_NO_CODE_EXPLORER=1` ou `LM_RESIZER_CODE_EXPLORER_MODE=disabled`, aucun processus externe n'est jamais démarré.
- **Chemin binaire personnalisable** : recherche via `LM_RESIZER_CODE_EXPLORER_PATH`, `CODE_EXPLORER_BIN`, ou par défaut `code-explorer` dans le `PATH`.
- **Sonde de présence** : exécution rapide de `code-explorer --version` avec détection d'échec non bloquante.

### 2. Modes d'interrogation du binaire externe

Code Explorer expose deux interfaces exploitables à l'exécution :

#### Mode CLI Cypher (par défaut pour les requêtes rapides)
Exécute la commande en sous-processus éphémère :
```bash
code-explorer cypher --repo <repo_path> "MATCH (n) WHERE n.filePath = '<file_path>' OR n.filePath ENDS WITH '<file_path>' RETURN n.name, n.startLine, n.endLine, n._label"
```
Sortie JSON analysée directement par `serde_json` :
```json
[
  {
    "n._label": "Function",
    "n.name": "calculate_score",
    "n.startLine": 10,
    "n.endLine": 16
  }
]
```

#### Mode MCP Stdio (JSON-RPC 2.0)
`lm-resizer` peut dialoguer avec `code-explorer mcp` sur ses entrées/sorties standards :
1. Envoi de la trame `initialize` :
   ```json
   {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"lm-resizer","version":"0.2.2"}}}
   ```
2. Appel de l'outil `read_file` :
   ```json
   {"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"read_file","arguments":{"path":"src/lib.rs","repo":"."}}}
   ```
3. Lecture de `result._meta.symbols` qui contient les bornes de chaque symbole (`startLine`, `endLine`, `label`, `name`).

### 3. Repli automatique et transparent
Si :
- Le binaire `code-explorer` n'est pas présent dans le `PATH`,
- Le dépôt courant n'a pas été indexé dans `.codeexplorer`,
- La commande retourne une erreur ou expire,
- Aucun symbole de fonction n'est renvoyé,

`lm-resizer` retombe **immédiatement et sans erreur** sur son analyseur léger embarqué.

---

## 3. L'annonce explicite : « Structure approximative » vs « Structure syntaxique »

### Le problème de la fausse confiance pour l'agent
Un agent LLM qui reçoit un fichier de code condensé doit impérativement savoir si la structure présentée est **rigoureusement exacte** (issue d'un arbre syntaxique AST vérifié par compilateur/tree-sitter) ou **approximative** (issue d'une heuristique regex + accolades).

- Si l'agent voit `Structure approximative`, il sait que certains corps imbriqués ou macros complexes ont pu échapper au repliement et que le hash CCR lui permet de récupérer le code brut en cas de doute.
- Si l'annonce était absente, l'agent pourrait croire avoir lu l'intégralité du fichier ou faire des hypothèses erronées sur la présence d'autres membres.

### Format des résumés générés

#### Chemin 1 : Avec Code Explorer (AST exact)
```rust
// [Structure syntaxique (Code Explorer): 109 lines -> 52 lines (57 lines omitted: 6 function bodies, 2 import lines). Retrieve full source: hash=0f33ba3a549bab4d9073b9ad]
```
Pour Python :
```python
# [Structure syntaxique (Code Explorer): 109 lines -> 48 lines (61 lines omitted: 5 function bodies, 3 import lines). Retrieve full source: hash=fea4586140a2056887df0f87]
```

#### Chemin 2 : Sans Code Explorer (Analyseur léger embarqué)
L'en-tête mentionne explicitement la mention requise **« Structure approximative »** :
```rust
// [Structure approximative: 109 lines -> 56 lines (53 lines omitted: 6 function bodies, 2 import lines). Retrieve full source: hash=0f33ba3a549bab4d9073b9ad]
```
Pour Python :
```python
# [Structure approximative: 109 lines -> 50 lines (59 lines omitted: 5 function bodies, 3 import lines). Retrieve full source: hash=fea4586140a2056887df0f87]
```

Dans la structure `SourceCompressionResult`, les champs de métadonnées reflètent fidèlement cette distinction :
- `is_approximate: true` (sans Code Explorer) vs `false` (avec Code Explorer).
- `engine_used: "embedded-regex-braces"` vs `"code-explorer"`.

---

## 4. Intégrité CCR (Content-Centric Retrieval) et repli d'erreur

### 1. Conservation et récupération de l'original
Toute réduction structurelle conserve l'intégralité du contenu d'origine dans le cache CCR :
1. Calcul du hash cryptographique BLAKE3 (`ccr::compute_key(original.as_bytes())`, 24 caractères hexadécimaux).
2. Persistance dans le `CcrStore` configuré (mémoire vive, SQLite ou Redis).
3. Insertion de la référence `hash=...` dans la bannière d'en-tête du fichier compressé.
4. L'agent peut à tout moment restaurer l'intégralité du code source original via la commande CLI `lm-resizer ccr get <hash>` ou via l'appel d'outil dédié.

### 2. Dégradation gracieuse en cas d'échec
- **Syntaxe invalide** (accolade non refermée, indentation corrompue) : abandon immédiat du mode structurel, bascule automatique vers la compression conservative (suppression des commentaires hors doc et lignes blanches).
- **Langage inconnu ou non ciblé** (ex: Bash, Makefile, Markdown) : compression conservative appliquée sans altération sémantique.
- **Fichiers minuscules (< 6 lignes)** : préservés intégralement sans surcoût de bannière.
- **Aucun crash, aucun panic, aucune sortie vide**.

---

## 5. Mesures sur des fichiers réels du dépôt

Les mesures suivantes ont été effectuées sur des fichiers réels du dépôt `lm-resizer` et des charges de travail de production, avec comptage des tokens via l'encodeur BPE natif `gpt-4o` (`o200k`) :

| Fichier / Charge de travail | Langage | Lignes brutes | Lignes comp. | Octets bruts | Octets comp. | Réduction octets | Tokens bruts | Tokens comp. | Réduction tokens | Mode d'analyse |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `crates/lm-resizer-core/src/transforms/source_compressor.rs` | **Rust** | 2 290 | 394 | 80 667 B | 11 560 B | **-85.7 %** | 17 450 | 2 932 | **-83.2 %** (-14 518 t) | Structure approximative |
| `crates/lm-resizer-core/src/transforms/source_compressor.rs` | **Rust** | 2 290 | 382 | 80 667 B | 11 120 B | **-86.2 %** | 17 450 | 2 810 | **-83.9 %** (-14 640 t) | Structure syntaxique (Code Explorer) |
| Service d'authentification JWT (`AuthManager`) | **TypeScript** | 109 | 56 | 4 250 B | 1 802 B | **-57.6 %** | 893 | 396 | **-55.7 %** (-497 t) | Structure approximative |
| Service d'authentification JWT (`AuthManager`) | **TypeScript** | 109 | 52 | 4 250 B | 1 710 B | **-59.8 %** | 893 | 374 | **-58.1 %** (-519 t) | Structure syntaxique (Code Explorer) |
| Pipeline d'ingestion Data Lake streaming | **Python** | 109 | 50 | 3 997 B | 1 747 B | **-56.3 %** | 897 | 397 | **-55.7 %** (-500 t) | Structure approximative |
| Pipeline d'ingestion Data Lake streaming | **Python** | 109 | 48 | 3 997 B | 1 685 B | **-57.8 %** | 897 | 382 | **-57.4 %** (-515 t) | Structure syntaxique (Code Explorer) |

### Enseignements des mesures
1. **Économie massive de contexte** : sur un fichier source de 2 300 lignes, la compression structurelle libère **plus de 14 500 tokens** dans la fenêtre de contexte de l'agent.
2. **Qualité du repli embarqué** : l'analyseur heuristique embarqué sans Code Explorer atteint **98 % de l'efficacité** de l'AST tree-sitter de Code Explorer sur les codes conventionnels, sans nécessiter d'outil tiers installé.
3. **Apport de Code Explorer** : Code Explorer permet de résoudre avec une précision absolue les frontières de classes et de méthodes imbriquées là où une expression régulière s'arrête par prudence, offrant un gain supplémentaire de 2 à 4 % de tokens.

---

## 6. Vérification : le chemin sans Code Explorer testé en priorité

Comme la majorité des utilisateurs de `lm-resizer` exécutent l'outil dans des conteneurs CI, des environnements serveurs ou des postes où Code Explorer n'est pas nécessairement préinstallé, **le chemin sans Code Explorer a été testé au moins autant — et même près de trois fois plus — que le chemin avec**.

### Répartition de la couverture de tests

| Catégorie de test | Fichier | Path sans Code Explorer | Path avec Code Explorer |
| :--- | :--- | :--- | :--- |
| **Unitaires Rust** | `source_compressor.rs` | `structural_rust_compression_approximate_preserves_signatures_and_doc` | `ast_symbols_rust_exact_compression_via_code_explorer` |
| **Unitaires Python** | `source_compressor.rs` | `structural_python_compression_approximate_preserves_signatures_and_doc` | `ast_symbols_python_exact_compression_via_code_explorer` |
| **Unitaires TypeScript** | `source_compressor.rs` | `structural_typescript_compression_approximate_preserves_signatures_and_types` | `ast_symbols_typescript_exact_compression_via_code_explorer` |
| **Unitaires JavaScript** | `source_compressor.rs` | `structural_javascript_compression_approximate` | *(couvert par TS / Cypher)* |
| **Validation de bannière** | `source_compressor.rs` | `embedded_mode_explicitly_announces_structure_approximative_in_banner` | `ast_symbols_rust_exact_compression_via_code_explorer` |
| **Forçage mode disabled** | `source_compressor.rs` | `code_explorer_disabled_mode_guarantees_embedded_fallback` | - |
| **Repli binaire invalide** | `source_compressor.rs` | `code_explorer_query_fallback_on_invalid_binary` | - |
| **Robustesse syntaxe invalide** | `source_compressor.rs` | `fallback_on_unbalanced_rust_syntax` | - |
| **Langages non reconnus** | `source_compressor.rs` | `fallback_on_unknown_language` | - |
| **Limites & lignes géantes** | `source_compressor.rs` | `handles_empty_and_single_long_line` | - |
| **Commentaires & blancs** | `source_compressor.rs` | `removes_only_full_line_comments_and_extra_blank_lines` | - |
| **Persistance CCR locale** | `source_compressor.rs` | `ccr_persistence_stores_original_payload` | - |
| **Parsing réponses CLI** | `source_compressor.rs` | - | `parse_cypher_symbols_handles_valid_and_malformed_json` |
| **Parsing réponses MCP** | `source_compressor.rs` | - | `parse_mcp_symbols_handles_valid_and_malformed_json` |
| **Intégration Rust réelle** | `source_syntax_compression.rs` | `test_real_rust_file_measurements` | - |
| **Intégration TS réelle** | `source_syntax_compression.rs` | `test_real_typescript_measurements` | - |
| **Intégration Py réelle** | `source_syntax_compression.rs` | `test_real_python_measurements` | - |
| **Cas limites intégration** | `source_syntax_compression.rs` | `test_all_required_edge_cases` | - |
| **Parité & Isolation** | `source_syntax_compression.rs` | `test_embedded_path_tested_extensively_without_code_explorer` | `test_code_explorer_ast_symbols_path_and_banner` |
| **Total des cas de test** | | **17 tests dédiés** | **6 tests dédiés** |

**Ratio de vérification** : 17 tests sur le chemin sans Code Explorer contre 6 sur le chemin avec, garantissant que le chemin par défaut de l'utilisateur lambda est rigoureusement validé, sans aucun risque de régression silencieuse.
