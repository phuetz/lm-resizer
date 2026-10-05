# LM Resizer 0.2.5

La 0.2.4 n'a jamais été publiée : cette version en reprend le contenu, ci-dessous, et y ajoute des hooks idempotents et un `rewrite-shell` qui n'enveloppe plus une sortie redirigée ou captée. Détail dans le CHANGELOG.

- Installation source documentée avec rustup et les prérequis C/C++ ; Rust minimal 1.91 pour le CLI (core/wasm : 1.86 ; toolchain épinglée : 1.95.0), y compris quand le Rust système est trop ancien. Les installateurs binaires vérifient SHA-256 et version ; le lecteur POSIX accepte aussi CRLF et les sommes sans saut de ligne final.
- Récupération de l'entrée originale du CLI avant transformation ; récupération CCR ajoutée à l'ABI C et au wrapper WASM. L'éviction protège les clés réinsérées. Le store de transmissions partagées utilise WAL et fournit des erreurs explicites.
- `exec` transmet stdin ; les arguments vides de la réécriture shell sont conservés. `rewrite-shell` laisse inchangée une commande dont la sortie est redirigée, tubée, substituée, fournie par un here-doc, ou interactive — recoller le suffixe écrivait la vue réduite. Les filtres conservent le contexte de recherche, les noms datés, les chemins TypeScript avec parenthèses, les fichiers Git commençant par `use` et les assertions Pytest. Les horodatages TRX invalides ne provoquent plus de panique.
- Statistiques CLI fondées sur `tiktoken-rs/o200k_base`, avec les estimations historiques distinguées. Démarrage Windows corrigé, skills et archives documentés, chemins personnels du constructeur neutralisés dans les binaires, garde de release étendue à tout le workspace.

Limites : TTL CCR natif de 30 minutes par défaut ; magasin WASM sans TTL, borné à 1000 entrées. Les gains du banc sont historiques et limités au corpus déclaré ; aucun gain universel ni coût fournisseur n'est promis. La purge automatique tee et la modification du filtre Git log rejetées ne sont pas incluses.

Sources des chiffres et détails : [CHANGELOG](https://github.com/phuetz/lm-resizer/blob/v0.2.5/CHANGELOG.md), [Rust minimal](https://github.com/phuetz/lm-resizer/blob/v0.2.5/Cargo.toml), [TTL et capacité CCR](https://github.com/phuetz/lm-resizer/blob/v0.2.5/crates/lm-resizer-core/src/ccr/mod.rs), [WASM](https://github.com/phuetz/lm-resizer/blob/v0.2.5/packages/wasm/README.md), [statistiques](https://github.com/phuetz/lm-resizer/blob/v0.2.5/docs/TOKEN-STATISTICS.md).
