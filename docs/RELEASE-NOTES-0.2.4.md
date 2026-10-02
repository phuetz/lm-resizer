# LM Resizer 0.2.4

Publication prévue le 8 octobre 2026. Cette préparation locale ne constitue pas une publication.

- Installation source documentée avec rustup et les prérequis C/C++ ; Rust minimal 1.86, y compris quand le Rust système est trop ancien. Les installateurs binaires vérifient SHA-256 et version ; le lecteur POSIX accepte aussi CRLF et les sommes sans saut de ligne final.
- Récupération de l'entrée originale du CLI avant transformation ; récupération CCR ajoutée à l'ABI C et au wrapper WASM. L'éviction protège les clés réinsérées. Le store de transmissions partagées utilise WAL et fournit des erreurs explicites.
- `exec` transmet stdin ; réécriture shell corrigée pour les redirections et les arguments vides. Les filtres conservent le contexte de recherche, les noms datés, les chemins TypeScript avec parenthèses, les fichiers Git commençant par `use` et les assertions Pytest. Les horodatages TRX invalides ne provoquent plus de panique.
- Statistiques CLI fondées sur `tiktoken-rs/o200k_base`, avec les estimations historiques distinguées. Démarrage Windows corrigé, skills et archives documentés, garde de release étendue à tout le workspace.

Limites : TTL CCR natif de 30 minutes par défaut ; magasin WASM sans TTL, borné à 1000 entrées. Les gains du banc sont historiques et limités au corpus déclaré ; aucun gain universel ni coût fournisseur n'est promis. La purge automatique tee et la modification du filtre Git log rejetées ne sont pas incluses.

Sources des chiffres et détails : [CHANGELOG](https://github.com/phuetz/lm-resizer/blob/v0.2.4/CHANGELOG.md), [Rust minimal](https://github.com/phuetz/lm-resizer/blob/v0.2.4/Cargo.toml), [TTL et capacité CCR](https://github.com/phuetz/lm-resizer/blob/v0.2.4/crates/lm-resizer-core/src/ccr/mod.rs), [WASM](https://github.com/phuetz/lm-resizer/blob/v0.2.4/packages/wasm/README.md), [statistiques](https://github.com/phuetz/lm-resizer/blob/v0.2.4/docs/TOKEN-STATISTICS.md).
