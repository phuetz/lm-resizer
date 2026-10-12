# Attributions des outils de comparaison

Ces outils servent uniquement de références de mesure dans `bench/`. Aucun de leurs codes n'est
inclus dans le produit ni dans l'archive de release ; le produit les mentionne seulement par
une désignation neutre (« outil de comparaison épinglé »).

- **RTK** (Apache-2.0), https://github.com/rtk-ai/rtk, version 0.50.0 épinglée par SHA-256 dans
  [`rtk-parity/reference.json`](rtk-parity/reference.json). LM Resizer s'en inspire pour l'idée de
  filtres de sorties de commandes ; ses filtres sont réécrits indépendamment en Rust
  ([`native/source-independence.md`](native/source-independence.md)). Le binaire de comparaison est
  construit hors du produit depuis l'archive épinglée.
- **Headroom** (Apache-2.0), version 0.39.1 avec ONNX Runtime 1.24.4, installé seulement dans
  l'environnement Python du banc. Les stratégies de vues structurées sont réécrites
  indépendamment en Rust.
