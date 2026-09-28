# Le conseiller de plages

## Le problème

Quand lm-resizer doit raccourcir un fichier source long, il coupe par lignes :
il garde le début. Or le début d'un fichier, ce sont les imports et l'en-tête —
rarement ce dont l'agent a besoin. Ce qui compte peut se trouver n'importe où.

Il faut donc que quelque chose qui *comprend* le code dise quelles plages
portent le sens.

## Pourquoi ce n'est pas une interface vers Code Explorer

Code Explorer sait le dire, et c'était la réponse évidente. C'est la mauvaise
forme d'interface, pour deux raisons qui ont pesé plus lourd que la commodité.

**lm-resizer doit fonctionner là où Code Explorer n'est pas installé**, ce qui
est le cas de la plupart des machines. Une interface qui porte le nom d'un outil
devient une interface qu'un seul outil peut satisfaire.

**La surface machine nécessaire n'existe pas encore.** Vérifié dans le dépôt de
référence en 0.2.1 : `--json` est offert par `doctor`, `hotspots`, `coupling` et
`ownership` — et pas par `query`, `context`, `impact` ni `analyze`, qui sont
justement ceux qui portent le sens structurel. Attendre cette surface bloquerait
tout le mécanisme.

L'interface est donc **un document ordinaire** : des plages de lignes avec des
poids. `ctags`, tree-sitter, un relevé de symboles, quelqu'un qui l'écrit à la
main — n'importe quoi peut en produire un. Code Explorer devient le meilleur
producteur au lieu d'être le seul, et la mécanique de rétention se construit et
se teste aujourd'hui.

Cette forme vient de la contre-lecture de Fable 5.1, qui connaît l'intérieur de
Code Explorer. Ma première version disait « Code Explorer décide ». La sienne
est meilleure.

## Le format

```json
{
  "advisor": "ctags",
  "ranges": [
    { "start_line": 42, "end_line": 88, "weight": 9.0, "label": "RestorePersistedJobs" },
    { "start_line": 120, "end_line": 134, "weight": 2.0 }
  ]
}
```

- `start_line` et `end_line` commencent à **1**, comme dans un éditeur et dans
  un compilateur, et sont **inclusives**.
- `weight` est relatif : plus haut compte plus. Absent, il vaut `1.0` —
  « compte, sans classement ».
- `label` ne sert qu'au diagnostic. Il ne décide de rien.
- `advisor` ne sert qu'au diagnostic **non plus** : une plage venue de `ctags`
  vaut exactement autant qu'une plage venue de Code Explorer. Le compresseur ne
  peut pas savoir qui a écrit le document, et c'est voulu.

## La règle qui rend le mécanisme sûr

**Un conseil ne fait que protéger. Il ne désigne jamais une ligne à supprimer.**

Une ligne dont personne n'a parlé est compressée par les règles ordinaires —
jamais supprimée *parce qu'*on n'en a pas parlé. Le silence d'un conseiller veut
dire « je ne sais pas », jamais « inutile ». C'est une exigence et non une
précaution : le graphe de Code Explorer manque des appels, notamment à travers
une interface ou une résolution dynamique, et une relation absente ne prouve pas
que le code est mort.

Conséquence vérifiée par un test : protéger une plage ne peut qu'**augmenter**
la taille du résultat par rapport à l'absence de conseil. Jamais la diminuer.

## Robustesse

Un conseiller est un programme extérieur. Sa sortie est une donnée, pas une
consigne, et elle peut être partiellement fausse.

- Une plage absurde — début à zéro, fin avant le début, poids non comparable —
  est **écartée seule**, sans faire tomber les autres.
- Un document illisible ne donne **aucun** conseil, ce qui ramène à la
  compression ligne à ligne. Ce n'est pas une erreur qui empêche de compresser.
- Un conseil vide se comporte exactement comme l'absence de conseil.
- Le classement est **déterministe** : à poids égal, ce sont les premières
  plages du fichier qui passent. Deux exécutions sur la même entrée donnent le
  même résultat, sans quoi un cache de compression ne vaut rien.

## Récupération

Ce qui est retiré reste récupérable par le mécanisme CCR existant, comme pour
les sorties de commandes. Une compression effective dépose sa clé ; une
compression qui n'a rien gagné n'en dépose pas.

C'est ce qui rend un taux de compression défendable. Une mesure d'Astra rappelle
pourquoi : une synthèse de diff avait omis 511 lignes, dont des corps
d'implémentation et de tests. Elle servait à naviguer, pas à valider. Ce que
lm-resizer retire doit rester récupérable, et le résumé doit le dire.

## Ce qui reste à faire

Le producteur `advice_from_symbols` pondère par **longueur de la plage**, ce qui
est un mauvais indicateur d'importance et doit être remplacé. Le vrai signal
serait le degré entrant et sortant dans le graphe, que Code Explorer n'expose
pas sous forme machine aujourd'hui.

D'où la demande faite à Astra : une commande neuve et étroite,
`code-explorer symbols <fichier> --json`, rendant nom, genre, plage de lignes et
degrés. Plus petite et plus stable qu'un `--format json` greffé sur `query`,
dont le classement change avec `--rerank`.
