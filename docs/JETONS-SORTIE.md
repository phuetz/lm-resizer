# Jetons de sortie : primitives et limites vérifiées

Le module `crates/lm-resizer-core/src/output.rs` fournit des primitives de
classification des tours, de consigne de concision (`steer_verbosity`) et de
routage d'effort (`route_effort`). Leur présence ne prouve pas une réduction
des jetons générés par un fournisseur.

Dans cette version, `src/main.rs` n'appelle pas ces primitives dans le proxy
HTTP. `serve` et `wrap` n'exposent ni `--no-verbosity` ni
`--no-effort-routing`. Consultez `lm-resizer serve --help` et
`lm-resizer wrap --help` pour les options réellement disponibles.

Les tests unitaires du module comprennent
`verbosity_steering_applied_once_idempotent`,
`verbosity_steering_can_be_disabled`,
`effort_routing_reduces_routine_and_keeps_non_routine`,
`effort_routing_anthropic_narrows_a_field_the_caller_already_sends`,
`effort_routing_anthropic_does_not_invent_the_field_by_default` et
`effort_routing_explicitly_refuses_unsupported_providers`.
Les tests de `crates/lm-resizer-core/tests/cache_control.rs` contrôlent le
calcul du préfixe gelé hors ligne. Ils ne mesurent pas le taux de cache du
fournisseur, les jetons générés, la réussite d'une tâche ou une facture.

`lm-resizer stats --markdown` affiche les entrées CCR, les commandes `exec`,
les octets économisés, une estimation des jetons économisés, les récupérations
et le tableau `Top Filters`. Il n'affiche ni économies de sortie mesurées ni
dollars. `src/provider_usage.rs` traite des comptes de jetons, pas des prix.

Les anciens chiffres de 75 jetons de concision et 800 jetons de réflexion
étaient des hypothèses, sans mesure fournisseur. Les tarifs et les sections
de statistiques autrefois décrits ici ne sont pas implémentés. Pour produire
une mesure de sortie, utilisez le protocole [A/B](MESURE-AB-SORTIE.md) et
conservez les réponses ainsi que les compteurs d'usage du fournisseur.
