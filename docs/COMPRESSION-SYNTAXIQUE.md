# Compression de code source consciente de la syntaxe

## 1. Constat initial et chemin d'exécution

### État antérieur de la compression de code source
Dans `crates/lm-resizer-core/src/transforms/source_compressor.rs`, la compression de code source (`SourceCompressor`) était purement conservative et ligne par ligne :
- Elle se contentait de supprimer les commentaires pleine ligne (`is_full_line_comment`, couvrant `//`, `///`, `#!`, `#`, `--`, `;`, `/* ... */`) et de restreindre les séquences de lignes vides consécutives à `max_blank_run: 1`.
- Si le résultat compressé n'était pas plus compact en octets que l'entrée, elle renvoyait le fichier original non modifié.
- **Limitation majeure** : face à un fichier long dont un agent LLM n'a besoin que de la structure générale (API, types, contrats, points d'entrée), une approche naïve tronque les lignes ou conserve les détails d'implémentation du haut du fichier au détriment des définitions et points d'entrée situés en bas. À l'inverse d'outils comme Headroom, le compresseur ne comprenait pas la structure syntaxique du code.

### Chemin d'appel existant
`SourceCompressor` est intégré sur le chemin canonique de `lm-resizer` sans créer de voie parallèle :
1. **Live Zone Dispatcher (`crates/lm-resizer-core/src/transforms/live_zone.rs`)** :
   - Pour tout bloc de payload de type `ContentType::SourceCode`, `dispatch_compressor` invoque directement `source_compressor().compress(text)`.
   - Lorsque la compression produit un texte modifié, `compress_one_block` encapsule le résultat avec le marqueur CCR (`maybe_inject_ccr_marker`), injecte la clé `<<ccr:HASH>>` et persiste le contenu original complet dans le `CcrStore` configuré.
2. **Pipeline d'orchestration (`crates/lm-resizer-core/src/lib.rs` / `pipeline`)** :
   - `SourceCompressor` implémente le trait `ReformatTransform` pour `ContentType::SourceCode` au sein du pipeline par défaut (`default_pipeline()`).
3. **Tests de dispatch et proxy** :
   - `crates/lm-resizer-core/tests/live_zone_dispatch.rs` valide que les `tool_result` de code source sont acheminés vers `source_compressor`.

---

## 2. Choix technique : analyse légère vs `tree-sitter`

### Vérification de l'état de `tree-sitter`
Une vérification intégrale du fichier de verrouillage `Cargo.lock` et des dépendances démontre que **`tree-sitter` n'est pas présent** dans le projet (0 occurrence directe ou transitive). La seule mention de `tree-sitter` dans tout le dépôt résidait dans un commentaire d'intention architecturale dans `crates/lm-resizer-core/src/signals/mod.rs` (ligne 20).

### Arbitrage architectural
Deux voies étaient envisageables pour structurer la compression :

| Critère | `tree-sitter` + grammaires dédiées | Analyseur structurel léger (machine à états + tokens) |
| :--- | :--- | :--- |
| **Dépendances externes** | `tree-sitter`, `tree-sitter-rust`, `tree-sitter-python`, `tree-sitter-typescript` | **Aucune dépendance nouvelle** (réutilise `regex = "1"` déjà présent) |
| **Chaîne de compilation** | Nécessite un compilateur C (`cc`) pour compiler `parser.c` de chaque grammaire | **100% Rust pur**, compilation instantanée |
| **Taille du binaire unique** | **+4 à +8 Mo** (tables d'états d'analyse LR statiques pour 3-4 langages) | **+34 Ko seulement (+0.13%)** |
| **Cible WASM** | Complexe pour `wasm32-unknown-unknown` (conflits runtime C) | **100% compatible WASM** (`crates/lm-resizer-wasm`) |
| **Performance d'exécution** | Construction d'un AST complet en mémoire avec allocations de nœuds | **Scan linéaire en un passage**, latence < 100 µs |
| **Résilience sur code tronqué** | Échecs / nœuds d'erreur fréquents sur des snippets partiels | **Dégradation gracieuse immédiate** vers le compresseur conservatif |

### Limites franches de l'analyseur structurel léger
Le choix d'un analyseur déterministe léger en Rust pur est un choix technique réfléchi et assumé. Ses limites sont clairement identifiées :
1. **Pas de résolution sémantique de types ni de macros** : les macros Rust complexes générant des blocs ou des syntaxe non standards ne sont pas expansées ; pour éviter toute altération sémantique, elles sont conservées telles quelles.
2. **Grammaires imbriquées exotiques** : en Python, les structures à indentation non conventionnelle ou les lambdas multilignes très spécifiques peuvent ne pas être résumées et sont conservées intactes.
3. **Expressions JSX / TSX complexes** : les balises JSX imbriquées avec interpolations complexes sont traitées prudemment pour ne pas risquer de déséquilibrer les accolades.
4. **Politique de tolérance zéro aux erreurs** : dès qu'un déséquilibre d'accolades ou une anomalie de bloc est détecté, l'analyseur abandonne le mode structurel et **retombe immédiatement sur la compression conservative**.

---

## 3. Ce qui a été construit

### 1. Langages pris en charge
La compression structurelle gère **Rust**, **Python**, et **TypeScript / JavaScript** (avec détection automatique via shebang et motifs syntaxiques caractéristiques sur les premières lignes).

### 2. Éléments conservés vs éléments résumés

#### Ce qui est conservé (intégrité sémantique pour l'agent) :
- **Signatures complètes** : paramètres, types, modificateurs (`pub`, `async`, `const`, `unsafe`, `extern`), clauses `where`, types de retour.
- **Noms publics et points d'entrée** : fonctions libres, méthodes d'API, `main()`, `if __name__ == "__main__":`, `export default`, etc.
- **Structure des types** : `struct`, `enum`, `trait`, `impl`, `interface`, `type` aliases, classes et propriétés d'instances.
- **Documentation d'en-tête** : commentaires `///` et `//!` (Rust), docstrings de module / classe / fonction `"""..."""` (Python), blocs JSDoc / TSDoc `/** ... */` (TypeScript/JS).
- **Décorateurs et attributs** : `#[derive(...)]`, `#[inline]`, `@dataclass`, `@property`, etc.

#### Ce qui est résumé et condensé :
- **Corps de fonctions et de méthodes** : remplacés par une indication explicite du nombre de lignes omises :
  - Rust / TS / JS : `/* ... [N lines omitted: function body] ... */`
  - Python : `# ... [N lines omitted: function body] ...` suivi de `...` pour préserver une syntaxe Python rigoureusement valide.
- **Imports volumineux** : les blocs de plus de 3 lignes consécutives conservent les 2 premières lignes et résument le reste :
  - Rust / TS / JS : `// ... [N import lines omitted] ...`
  - Python : `# ... [N import lines omitted] ...`

### 3. Traçabilité et intégration CCR (Compress-Cache-Retrieve)
Pour éviter tout "piège" où un agent LLM croirait avoir lu l'intégralité d'un fichier tronqué sans le savoir :
1. **Bannière d'en-tête explicite** : chaque fichier compressé structurellement reçoit en tête (ou sous le shebang) une bannière mentionnant la réduction, le nombre de corps et d'imports omis, et le hash BLAKE3 (24 hex chars) :
   ```rust
   // [Structure-compressed: 1442 lines -> 236 lines (1228 lines omitted: 26 function bodies, 2 import lines). Retrieve full source: hash=578fc199792cd075034da9e0]
   ```
2. **Mentions locales** : chaque bloc élidé stipule le nombre exact de lignes retirées.
3. **Persistance CCR** : le hash calculé (`ccr::compute_key(original.as_bytes())`) est identique à celui utilisé par le mécanisme CCR global. Lorsqu'un store est fourni (`compress_with_store` ou via `live_zone`), le payload original complet y est stocké sous cette clé. L'agent peut ainsi restaurer les corps de fonction omis à tout moment via l'outil `lm_resizer_retrieve(hash="...")`.

### 4. Robustesse et repli (Fallback)
- **Langage inconnu (Go, C, Bash, HTML, Markdown, etc.)** : repli transparent vers le compactage conservatif des commentaires non-doc et lignes blanches.
- **Code syntaxiquement invalide** : accolades non fermées, syntaxe déséquilibrée -> repli conservatif immédiat, aucune panique, aucune sortie vide.
- **Fichier vide** : renvoie une chaîne vide sans erreur.
- **Ligne unique géante** : traitée sans allocation excessive ni boucle infinie.

---

## 4. Mesures réelles et gains obtenus

Les mesures ci-dessous ont été relevées via la suite de tests d'intégration `crates/lm-resizer-core/tests/source_syntax_compression.rs` en utilisant le tokenizer natif BPE OpenAI (`gpt-4o` / `o200k`) :

| Fichier / Charge de travail | Langage | Lignes initiales | Lignes compressées | Octets initiaux | Octets compressés | Réduction octets | Tokens initiaux | Tokens compressés | Économie de tokens |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `crates/lm-resizer-core/src/transforms/source_compressor.rs` | **Rust** | 1 442 | 236 | 50 287 B | 6 977 B | **-86.1 %** | 10 773 | 1 824 | **-83.1 %** (-8 949 tokens) |
| Service d'authentification & JWT Express | **TypeScript** | 109 | 56 | 4 250 B | 1 799 B | **-57.7 %** | 893 | 396 | **-55.7 %** (-497 tokens) |
| Pipeline d'ingestion & Data Lake streaming | **Python** | 109 | 50 | 3 997 B | 1 744 B | **-56.4 %** | 897 | 397 | **-55.7 %** (-500 tokens) |

Sur un fichier réel du dépôt de 50 Ko, la compression consciente de la structure permet d'économiser **près de 9 000 tokens** tout en préservant 100% des signatures de fonctions, des traits, des structures de types et de la documentation.

---

## 5. Impact sur le binaire et validation

### Taille du binaire unique (`target/release/lm-resizer`)
- **Taille avant modification** : `25 497 048 octets` (24.31 MiB)
- **Taille après modification** : `25 531 496 octets` (24.35 MiB)
- **Surcoût total** : `+34 448 octets` (**+0.13 %**, environ 33.6 Ko)

À titre de comparaison, l'intégration de `tree-sitter` et de ses parseurs C aurait alourdi le binaire de plus de 4 000 Ko (+16 %).

### Validation de la suite de tests et conformité Clippy
- `cargo test --release` : **100 % vert** (846 tests unitaires de `lm-resizer-core`, 88 tests d'intégration du binaire `lm-resizer`, et l'ensemble des tests d'intégration sans régression).
- `cargo clippy --all-targets` : **zéro nouvel avertissement** (conservation stricte de l'état initial du dépôt).
- Compatibilité validée pour les environnements natifs et WASM (`lm-resizer-wasm`).
