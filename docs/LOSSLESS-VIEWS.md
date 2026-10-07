# Archives réversibles et vues lisibles

**Les formats ci-dessous sont historiques.** Les filtres automatiques ne les
émettent plus : une représentation réversible pouvait afficher de faux numéros,
des auteurs implicites incorrects ou des chemins échappés trompeurs. Les vues
courantes conservent les faits littéraux. Les seuls résumés concernés ici sont
les réussites de test explicitement reconnues, avec bilan conservé.

`lm-resizer expand --input vue.txt` sert à lire une ancienne archive encodée.
Ce décodage ne constitue pas un contrôle de visibilité. Une vue modifiée affiche
`[raw: …]` et `Original bytes: lm-resizer tee read …`. `exec` préserve les octets
non UTF-8 dans tee, même lorsque le texte affiché doit les noter `\xNN`.
Les comptes de jetons de ces captures binaires portent sur le rendu textuel
explicitement échappé; ils ne prétendent pas tokenizer des octets invalides.

## LMR-LINES/2 et LMR-LINES/3

Le numéro d'une ligne est son index dans le texte **décodé**, à partir de zéro.
Les lignes littérales commencent par `\` si leur premier caractère est un
caractère de contrôle du format (`@`, `&`, `=`, `!` ou `\`).

| Enregistrement | Interprétation |
|---|---|
| `@"préfixe"` | Définir le préfixe JSON des lignes littérales suivantes |
| `&N` | Répéter la ligne décodée N |
| `=N` | N répétitions supplémentaires de la dernière ligne |
| `!0` / `!1` | Fin de vue, sans / avec LF final |
| `@N` (v3) | Réutiliser un préfixe déjà défini, indexé depuis zéro |
| `@N:"suffixe"` (v3) | Définir un nouveau préfixe par extension du préfixe N |
| `&N:C` (v3) | Répéter C lignes déjà décodées à partir de N |

Une sélection `@N` ne crée pas une nouvelle définition. Les blocs ne peuvent
pas référencer de lignes futures. Ordre, répétitions, espaces et CR sont
préservés. Les références invalides et expansions excessives sont refusées.

## LMR-TEXT/1

La seconde ligne est un objet JSON associant au maximum 52 lettres ASCII aux
identifiants ou préfixes d'URL répétés. Dans le corps, `~a` représente exactement
la valeur associée à `a`; `~~` représente un tilde littéral. Le dernier
enregistrement `!0` ou `!1` porte le LF final, comme pour LMR-LINES.
Toutes les définitions restent lisibles dans l'en-tête. Les URL complètes et
identifiants originaux sont reconstruits, sans troncature.

## Contrôles actuels

`bench/real/run.py` contrôle les faits dans la vue **sans appeler son décodeur**.
Le décodeur indépendant reste pour les tests des archives et les quatre
mutations de `test_oracle.py`. `check_views.py` exige l’égalité littérale pour
les fixtures et les 80 cas adverses. `capture_visible.py` ajoute des exécutions
réelles de Docker, tsc, ESLint, Jest, pytest et Cargo, avec leurs échecs attendus.
Voir [les résultats et limites](../bench/real/visible/RESULTATS.md).
