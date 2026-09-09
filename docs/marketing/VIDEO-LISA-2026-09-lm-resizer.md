# VIDEO — lm-resizer 0.2.1

Chaîne **Lisa IA** · publication 09/09/2026 · voix ElevenLabs · avatar Lisa  
Durée cible voix : 10 à 12 min · débit visé ~160 mots/min · **le texte à l'écran = le script, mot pour mot**  
Citations `[fichier:ligne]` devant chaque chiffre : à retirer à la relecture.

Les 5 cases

| Case | Valeur |
| --- | --- |
| STAR | lm-resizer [LINKEDIN-2026-09-lm-resizer.md:1] 0.2.1 |
| CHIFFRE | [README.md:8] 398 commandes, [README.md:8] 1,23 Mo → [README.md:8] 372 Ko, [README.md:8] 222 247 tokens |
| ENNEMI | le bruit de commande qui noie l'erreur |
| MÉTAPHORE | un filtre entre la commande et le modèle — le signal reste, le brut se range |
| TWIST (~70 %) | le paquet npm ne compresse pas les logs : seulement du JSON ; le reste est dans le binaire Rust |

---

## 1) Titre

**Titre retenu (51 caractères)**

```
lm-resizer vient de couper 222 247 tokens d'un test
```

**Repli 1 (41 caractères)**

```
Un cargo test : 1,23 Mo deviennent 372 Ko
```

**Repli 2 (40 caractères)**

```
lm-resizer 0.2.1 vient d'arriver sur npm
```

---

## 2) Miniature

**Texte (4 mots)** : `222 247 TOKENS`

**Description visuelle.** Fond noir charbon. À gauche, capitales jaunes outline noir : `222 247 TOKENS` ; sous-ligne blanche plus petite : `1,23 Mo → 372 Ko`. À droite, Lisa détourée, regard caméra, une main ouverte vers le chiffre. En bas, petit mot `lm-resizer`. Contraste fort, lisible à 320 px. Pas de prix, pas de visage d'auteur, pas de logo GitHub dominant.

---

## 3) Description YouTube (~150 mots)

```
Contenu synthétique : présentatrice générée par IA. Lisa parle pour le studio Agile Up. Les faits sont sourcés.

Un agent de code passe une part énorme de sa fenêtre à lire du bruit : tests, logs npm, diffs, listings. lm-resizer se met entre la commande et le modèle et ne garde que le signal.

Un cargo test à travers lui : 398 commandes, 1,23 Mo → 372 Ko, 222 247 tokens économisés. Les erreurs, les chemins et le résumé restent. La sortie complète reste récupérable.

Rust. Enveloppe de commande, proxy HTTP, serveur MCP, hooks Claude Code et Codex. Compression consciente de la question. Le module WebAssembly est sur npm : trois lignes pour compresser un JSON depuis Node. Ce paquet-là ne compresse pas les logs — ça, c'est le binaire.

npm install @phuetz/lm-resizer
https://www.npmjs.com/package/@phuetz/lm-resizer
https://github.com/phuetz/lm-resizer
https://phuetz.github.io/lm-resizer/

Abonne-toi : youtube.com/@lisaiafr
#LisaIA #lmresizer #ClaudeCode #Codex #Rust #MCP
```

---

## 4) Script — plans numérotés

Rythme : cold open 2–4 s ; ensuite un visuel toutes les 3–5 s ; Lisa en insert aux plans 1, 9, 55, 60, 86 (jamais plus de 8 s d'affilée). Karaoké = **Dit**, mot pour mot. Sources affichées en bas d'écran au format `README · Lxx` quand un chiffre est dit.

Horloge cumulée en tête de plan. Total visé **~11:10**.

### Acte 0 — Hook et promesse (0:00–0:20)

#### Plan 1 · 0:00–0:04 · 4 s · 🎥 Lisa #1
- **Dit :** Un `cargo test` à travers lm-resizer [README.md:8].
- **Écran :** Lisa regard caméra + PNG `docs/lm-resizer-hero.png`. Karaoké identique.

#### Plan 2 · 0:04–0:08 · 4 s
- **Dit :** [README.md:8] 398 commandes. [README.md:8] 1,23 Mo deviennent [README.md:8] 372 Ko.
- **Écran :** `1,23 Mo → 372 Ko` en gros. Carton `398`.

#### Plan 3 · 0:08–0:12 · 4 s
- **Dit :** [README.md:8] 222 247 tokens économisés.
- **Écran :** `222 247` plein cadre. Source `README · L8`.

#### Plan 4 · 0:12–0:16 · 4 s
- **Dit :** Le signal reste. Le bruit tombe. La sortie complète reste récupérable [README.md:9].
- **Écran :** trois pastilles SIGNAL / BRUIT / RÉCUPÉRABLE.

#### Plan 5 · 0:16–0:20 · 4 s
- **Dit :** Aujourd'hui je te montre où le bruit mange la fenêtre, comment on le coupe, et ce que le paquet npm ne fait pas.
- **Écran :** trois cartons BRUIT / COUPE / LIMITE npm.

---

### Acte 1 — Le bruit remplit la fenêtre (0:20–2:20)

#### Plan 6 · 0:20–0:26 · 6 s
- **Dit :** Un agent de code passe une part énorme de sa fenêtre de contexte à lire du bruit [README.md:17].
- **Écran :** jauge de contexte qui se remplit de logs.

#### Plan 7 · 0:26–0:32 · 6 s
- **Dit :** Sorties de tests. Logs de npm. Diffs. Listings [LINKEDIN-2026-09-lm-resizer.md:11].
- **Écran :** quatre captures, jump cut 1,5 s.

#### Plan 8 · 0:32–0:38 · 6 s
- **Dit :** `cargo test`. `npm test`. `git diff`. `rg`. Linters. Gestionnaires de paquets. Appels d'API [README.md:21].
- **Écran :** bandeau de commandes.

#### Plan 9 · 0:38–0:44 · 6 s · 🎥 Lisa #2
- **Dit :** Des milliers de lignes répétées, de faible valeur, ou structurellement bruyantes [README.md:23].
- **Écran :** Lisa + scroll de log identique.

#### Plan 10 · 0:44–0:50 · 6 s
- **Dit :** Envoyer tout ça au modèle remplit la fenêtre. Et ça peut cacher l'erreur qui compte [README.md:24].
- **Écran :** une ligne `ERROR` noyée, puis extraite.

#### Plan 11 · 0:50–0:58 · 8 s
- **Dit :** Une plus grande fenêtre ne rend pas ce bruit inoffensif [README.md:29]. Elle le rend lourd trois fois : à la latence, à l'attention du modèle, et à la densité du signal [README.md:31].
- **Écran :** trois colonnes LATENCE / ATTENTION / DENSITÉ. Pas de tarif.

#### Plan 12 · 0:58–1:06 · 8 s
- **Dit :** Les modèles raisonnent moins bien quand le signal est enterré au milieu [README.md:32]. On appelle ça se perdre au milieu. lm-resizer travaille la densité du signal, pas seulement une limite de taille [README.md:33].
- **Écran :** schéma signal au centre, bruit aux bords.

#### Plan 13 · 1:06–1:14 · 8 s
- **Dit :** Il peut aussi compresser en tenant compte de ta question : quand il faut couper, il garde les lignes qui répondent à ce que tu demandes [README.md:35].
- **Écran :** query `error` → les lignes error survivent.

#### Plan 14 · 1:14–1:22 · 8 s
- **Dit :** Agnostique du fournisseur. Validé de bout en bout le [README.md:41] 23 juin [README.md:41] 2026, devant Mistral, Ollama en local, DeepSeek, OpenRouter, et xAI / Grok [README.md:41].
- **Écran :** cinq noms, date `2026-06-23`.

#### Plan 15 · 1:22–1:30 · 8 s
- **Dit :** Devant Mistral, une charge d'outil bruyante est passée d'environ [README.md:43] 5,8 Ko à [README.md:43] 2,8 Ko, puis le modèle a répondu [README.md:43].
- **Écran :** `~5,8 Ko → 2,8 Ko` puis bulle de réponse.

#### Plan 16 · 1:30–1:38 · 8 s
- **Dit :** Toute autre API compatible OpenAI ou Anthropic passe par le même chemin [README.md:44].
- **Écran :** routes `/v1/chat/completions` · `/v1/messages` [README.md:415].

#### Plan 17 · 1:38–1:46 · 8 s
- **Dit :** lm-resizer fait partie d'une chaîne. Code Explorer donne à l'agent une carte du dépôt. lm-resizer le protège du bruit d'exécution. Code Buddy orchestre le travail [README.md:51].
- **Écran :** trois briques, lm-resizer au centre.

#### Plan 18 · 1:46–1:54 · 8 s
- **Dit :** Le but n'est pas seulement de compresser. C'est de travailler plus propre : de la structure, moins de bruit, et la preuve complète récupérable quand il le faut [README.md:57].
- **Écran :** STRUCTURE / MOINS DE BRUIT / PREUVE.

#### Plan 19 · 1:54–2:02 · 8 s
- **Dit :** C'est un moteur Rust, autonome, léger à l'exécution. Pas de collecteur de télémétrie, pas de tableau de bord forcé, pas d'abonné de traces allumé par le binaire [README.md:103].
- **Écran :** `no telemetry collector`.

#### Plan 20 · 2:02–2:10 · 8 s
- **Dit :** Pas de runtime Python dans ce dépôt. Build, tests, MCP, CLI, proxy : Cargo et binaires Rust seulement [README.md:107].
- **Écran :** tampon `Rust only`.

#### Plan 21 · 2:10–2:20 · 10 s
- **Dit :** La classification ML est optionnelle, et éteinte par défaut [README.md:110]. Le chemin normal, c'est de la détection locale déterministe. Pour allumer Magika, il faut le feature `magika` à la compilation, et `LM_RESIZER_ENABLE_MAGIKA=1` au runtime [README.md:111].
- **Écran :** Magika OFF par défaut.

---

### Acte 2 — La preuve : exec, le filtre, la récupération (2:20–4:50)

#### Plan 22 · 2:20–2:26 · 6 s
- **Dit :** Revenons au chiffre. Une exécution de `cargo test` à travers lm-resizer [README.md:8].
- **Écran :** `docs/lm-resizer-hero.png`.

#### Plan 23 · 2:26–2:32 · 6 s
- **Dit :** [README.md:8] 398 commandes. [README.md:8] 1,23 Mo. [README.md:8] 372 Ko. [README.md:8] 222 247 tokens.
- **Écran :** quatre nombres, un par cut.

#### Plan 24 · 2:32–2:38 · 6 s
- **Dit :** Les erreurs restent. Les chemins de fichiers restent. Le résumé reste [LINKEDIN-2026-09-lm-resizer.md:13].
- **Écran :** trois lignes gardées, surlignées.

#### Plan 25 · 2:38–2:44 · 6 s
- **Dit :** La sortie complète reste récupérable si l'agent en a besoin [README.md:9].
- **Écran :** hint `[full output: …]` [README.md:279].

#### Plan 26 · 2:44–2:52 · 8 s
- **Dit :** `lm-resizer exec -- git status`. `lm-resizer exec --json -- cargo test` [README.md:176].
- **Écran :** ces deux commandes, police mono.

#### Plan 27 · 2:52–3:00 · 8 s
- **Dit :** `exec` lance une commande, applique un filtre de sortie inspiré RTK pour les familles bruyantes : `git`, `cargo`, `rg`, `vitest` / `jest` — en direct ou via npx, pnpm, yarn, bunx — et les listings de dossiers [README.md:215].
- **Écran :** familles de commandes en bandeau.

#### Plan 28 · 3:00–3:08 · 8 s
- **Dit :** Puis il envoie le texte filtré dans le pipeline de compression normal [README.md:218].
- **Écran :** FILTER → COMPRESS → AGENT.

#### Plan 29 · 3:08–3:16 · 8 s
- **Dit :** C'est un wrapper CLI explicite. Pas un crochet de shell automatique [README.md:218].
- **Écran :** tampon `explicit wrapper ≠ auto hook`.

#### Plan 30 · 3:16–3:24 · 8 s
- **Dit :** Le filtre des test-runners garde les fichiers et les tests en échec, les diffs d'assertion, les frames de pile, et les compteurs finaux [README.md:219].
- **Écran :** FAIL + assertion + stack + counters.

#### Plan 31 · 3:24–3:32 · 8 s
- **Dit :** Un run qui passe se replie sur les lignes de résumé [README.md:221]. Dans la télémétrie agent, la sortie des test-runners est le plus gros puits à tokens [README.md:221].
- **Écran :** PASS → summary only. Carton `plus gros puits`.

#### Plan 32 · 3:32–3:40 · 8 s
- **Dit :** `--stream` pour une commande longue : tu gardes la sortie enfant en direct ; lm-resizer capture le flux et émet le résultat filtré après la sortie de l'enfant [README.md:224].
- **Écran :** `lm-resizer exec --stream -- cargo test` [README.md:178].

#### Plan 33 · 3:40–3:48 · 8 s
- **Dit :** `tool-output` est le pendant côté agent. Il ne lance jamais la commande [README.md:227].
- **Écran :** tampon `NEVER EXECUTES`.

#### Plan 34 · 3:48–3:56 · 8 s
- **Dit :** Un hôte comme Code Buddy valide, approuve, exécute dans son bac, puis envoie la sortie inerte à `tool-output` [README.md:228].
- **Écran :** sandbox hôte → pipe → lm-resizer.

#### Plan 35 · 3:56–4:04 · 8 s
- **Dit :** `--command` sert seulement à choisir le même filtre sémantique que `exec` [README.md:231]. Budget de jetons optionnel, conscient de la question [README.md:231].
- **Écran :** `--command "cargo test" --token-budget 2000` [README.md:179].

#### Plan 36 · 4:04–4:12 · 8 s
- **Dit :** L'original complet est rangé sous un hash de récupération CCR [README.md:232]. Si le candidat ne tient pas les seuils d'économie, on rend l'original exact [README.md:233].
- **Écran :** `--min-savings-bytes` · `--min-savings-ratio`.

#### Plan 37 · 4:12–4:20 · 8 s
- **Dit :** L'écriture de récupération est vérifiée avant d'annoncer le hash. Si le backend CCR ne peut pas relire l'original, lm-resizer rend le brut exact [README.md:234].
- **Écran :** CCR fail → RAW.

#### Plan 38 · 4:20–4:28 · 8 s
- **Dit :** La compression ne rend jamais un résultat d'outil plus gros [README.md:239].
- **Écran :** tampon `no-growth`.

#### Plan 39 · 4:28–4:36 · 8 s
- **Dit :** `retrieve` relit un hash CCR. `stats` montre les compteurs. `doctor` diagnostique l'install locale [README.md:202].
- **Écran :** `lm-resizer retrieve <hash>` · `stats` · `doctor --json`.

#### Plan 40 · 4:36–4:44 · 8 s
- **Dit :** Quand la sortie est grosse, ou que l'enfant échoue, `exec` range le brut et ajoute un indice `[full output: …]` [README.md:278]. `LM_RESIZER_TEE=0` éteint cette récupération [README.md:280].
- **Écran :** tee hint à l'écran.

#### Plan 41 · 4:44–4:50 · 6 s
- **Dit :** `tee list`, `tee read`, `tee purge` gèrent ces sorties brutes [README.md:282].
- **Écran :** trois sous-commandes tee.

---

### Acte 3 — Hooks, MCP, proxy (4:50–7:10)

#### Plan 42 · 4:50–4:56 · 6 s
- **Dit :** Quatre façons de l'utiliser. Enveloppe de commande. Proxy HTTP. Serveur MCP. Hooks pour Claude Code et Codex [LINKEDIN-2026-09-lm-resizer.md:15].
- **Écran :** quatre portes.

#### Plan 43 · 4:56–5:04 · 8 s
- **Dit :** `lm-resizer mcp`. `lm-resizer install --client claude --scope project`. Pareil pour Codex, ou `all` [README.md:349].
- **Écran :** commandes d'install MCP.

#### Plan 44 · 5:04–5:12 · 8 s
- **Dit :** Quatre outils exposés : `lm_resizer_compress`, `lm_resizer_tool_output`, `lm_resizer_retrieve`, `lm_resizer_stats` [README.md:360].
- **Écran :** liste des 4 tools, `tool_output` annoté `never executes`.

#### Plan 45 · 5:12–5:20 · 8 s
- **Dit :** `init-native-hooks` écrit la vraie config Claude Code / Codex : un PreToolUse qui substitue une commande Bash supportée par `lm-resizer exec --`, et un PostToolUse qui enregistre les économies [README.md:311].
- **Écran :** PreToolUse rewrite · PostToolUse telemetry.

#### Plan 46 · 5:20–5:28 · 8 s
- **Dit :** Le rewrite ne bloque jamais. Une commande non supportée ou illisible ne dit rien et tourne brute. Le crochet refuse de se ré-envelopper lui-même [README.md:316].
- **Écran :** unsupported → raw.

#### Plan 47 · 5:28–5:36 · 8 s
- **Dit :** `rewrite` ne lance rien. Il dit si la commande est supportée, et imprime l'équivalent `lm-resizer exec -- …` [README.md:245].
- **Écran :** `lm-resizer rewrite -- git status` [README.md:180].

#### Plan 48 · 5:36–5:44 · 8 s
- **Dit :** `rewrite-shell` fait la même chose sur une ligne complète. Il peut réécrire des segments indépendants joints par `&&`, `||`, ou `;`. Il ne touche pas un pipeline ni une redirection [README.md:249].
- **Écran :** pipeline / redirection = intact.

#### Plan 49 · 5:44–5:52 · 8 s
- **Dit :** `init-shims` pose des shims PATH optionnels. Tu mets ce dossier en tête du `PATH` pour router les commandes bruyantes connues [README.md:319].
- **Écran :** `.lm-resizer/shims` devant PATH.

#### Plan 50 · 5:52–6:00 · 8 s
- **Dit :** Filtres déclaratifs dans `.lm-resizer/filters.toml`, puis `trust-filters` pour approuver le hash du fichier [README.md:255].
- **Écran :** `trust-filters` · hash.

#### Plan 51 · 6:00–6:08 · 8 s
- **Dit :** Filtres intégrés : Terraform / OpenTofu, Docker / Podman, `systemctl`, installs de paquets, Homebrew, `make`, GitHub CLI, Go, .NET, linters Python, JVM, gestionnaires Python, qualité JS, logs Docker, Kubernetes, AWS CLI [README.md:257].
- **Écran :** nuage de noms, 3 s par groupe.

#### Plan 52 · 6:08–6:16 · 8 s
- **Dit :** `verify-filters` valide la syntaxe, refuse les champs inconnus, joue les `[[tests]]` inline. `trust-filters` refuse un fichier dont les tests inline échouent [README.md:265].
- **Écran :** tests inline FAIL → pas de trust.

#### Plan 53 · 6:16–6:24 · 8 s
- **Dit :** `lm-resizer serve --bind 127.0.0.1:8787` [README.md:405]. Santé, compress, tool-output, retrieve, stats, et les routes `/v1/*` compatibles fournisseur [README.md:410].
- **Écran :** liste d'endpoints.

#### Plan 54 · 6:24–6:32 · 8 s
- **Dit :** Avec `--upstream`, la requête compressée part vers le fournisseur. Sans upstream, le serveur rend un aperçu : requête compressée plus stats [README.md:428].
- **Écran :** preview vs forward.

#### Plan 55 · 6:32–6:40 · 8 s · 🎥 Lisa #3
- **Dit :** `serve --dashboard` allume une vue HTML locale sur le store et les compteurs `exec`. Éteint par défaut. Ça ne démarre pas un collecteur de télémétrie [README.md:337].
- **Écran :** Lisa + carton dashboard OFF by default.

#### Plan 56 · 6:40–6:48 · 8 s
- **Dit :** `discover` scanne des fichiers de session, estime ce que `exec` aurait économisé. Il n'exécute aucune commande [README.md:284].
- **Écran :** `lm-resizer discover ~/.claude/projects --recursive --markdown` [README.md:193].

#### Plan 57 · 6:48–6:56 · 8 s
- **Dit :** `eval` est un harnais léger sur les mêmes fixtures : pass / warn, comptes, candidats, économies estimées, pour la CI ou les notes de version [README.md:291].
- **Écran :** `lm-resizer eval fixtures/sessions --recursive --markdown` [README.md:194].

#### Plan 58 · 6:56–7:04 · 8 s
- **Dit :** `learn` s'appuie sur `discover` plus l'historique `exec` local. Il écrit une mémoire durable, et peut poser un bloc de guidage réversible dans `AGENTS.md` ou `CLAUDE.md`, distinct du bloc de hooks [README.md:300].
- **Écran :** learning block ≠ hook block.

#### Plan 59 · 7:04–7:10 · 6 s
- **Dit :** Store CCR par défaut : sous Linux et macOS, `$XDG_STATE_HOME/lm-resizer/ccr.sqlite3` ou `$HOME/lm-resizer/ccr.sqlite3` [README.md:332].
- **Écran :** chemin Linux du store.

---

### CTA · ~70 % (7:10–7:40)

#### Plan 60 · 7:10–7:18 · 8 s · 🎥 Lisa #4
- **Dit :** Nouveau aujourd'hui : le module WebAssembly est sur npm, avec une vraie fiche et un chargeur intégré. [LINKEDIN-2026-09-lm-resizer.md:17] Trois lignes pour compresser un JSON depuis Node.
- **Écran :** Lisa + `npm install @phuetz/lm-resizer` [packages/wasm/README.md:18]. Bouton S'ABONNER.

#### Plan 61 · 7:18–7:26 · 8 s
- **Dit :** Lien npm en description. Site : phuetz.github.io/lm-resizer [README.md:12]. Abonne-toi, la prochaine vidéo c'est Code Explorer — la carte que l'agent interroge avant d'ouvrir des dizaines de fichiers.
- **Écran :** npm + site + S'ABONNER.

#### Plan 62 · 7:26–7:40 · 14 s
- **Dit :** Usage Node : tu importes `initLmResizerWasmFromPackage`, tu attends le module, tu appelles `compressJson` [packages/wasm/README.md:26]. Un deuxième argument optionnel, la question, pour garder ce qui s'y rapporte [packages/wasm/README.md:31].
- **Écran :** extrait JS du README wasm ; karaoké = phrase dite.

---

### Acte 4 — Le paquet npm, et ce qu'il ne fait pas (7:40–9:40)

#### Plan 63 · 7:40–7:48 · 8 s
- **Dit :** Le paquet livre le `lm_resizer_wasm.wasm` compilé, environ [packages/wasm/README.md:21] 2,6 Mo. Rien à construire [packages/wasm/README.md:21].
- **Écran :** `~2,6 Mo`.

#### Plan 64 · 7:48–7:56 · 8 s
- **Dit :** Plancher Node : [packages/wasm/README.md:8] 18.
- **Écran :** `Node ≥ 18`.

#### Plan 65 · 7:56–8:04 · 8 s
- **Dit :** Le fumigène du paquet : [packages/wasm/README.md:59] 3 161 octets deviennent [packages/wasm/README.md:59] 1 222.
- **Écran :** `3 161 → 1 222`. Source `packages/wasm/README · L59`.

#### Plan 66 · 8:04–8:12 · 8 s
- **Dit :** L'entrée doit être du JSON. Les grands tableaux d'objets semblables, le nesting profond, les clés répétées : ça se compresse bien [packages/wasm/README.md:58].
- **Écran :** JSON array → crushed.

#### Plan 67 · 8:12–8:20 · 8 s
- **Dit :** Un document petit, ou déjà dense, peut revenir inchangé, avec `bytes_saved: 0` et `steps_applied` vide. C'est la réponse honnête, pas une erreur [packages/wasm/README.md:59].
- **Écran :** `bytes_saved: 0` · pas une erreur.

#### Plan 68 · 8:20–8:28 · 8 s
- **Dit :** La compression du texte brut et des logs — sorties npm, cargo, pytest, traces, listings — est dans le CLI Rust. Pas dans ce build wasm [packages/wasm/README.md:62].
- **Écran :** split WASM = JSON · CLI = logs.

#### Plan 69 · 8:28–8:36 · 8 s
- **Dit :** Fonction pure. Pas d'entrées-sorties. Pas de réseau. Le module alloue dans sa propre mémoire linéaire [packages/wasm/README.md:64].
- **Écran :** `no I/O · no network`.

#### Plan 70 · 8:36–8:44 · 8 s
- **Dit :** C'est le twist, et je le dis net : si tu installes le paquet npm en croyant filtrer `cargo test`, tu n'as pas le bon outil. Il te faut le binaire [LINKEDIN-2026-09-lm-resizer.md:57].
- **Écran :** npm barré pour `cargo test`, binaire coché.

#### Plan 71 · 8:44–8:52 · 8 s
- **Dit :** Le binaire, lui, a le pipeline complet : détection de contenu, minification JSON, SmartCrusher, rétention consciente de la question, compression de logs, offload de diffs, compression conservative de source, store CCR SQLite [README.md:90].
- **Écran :** liste des primitives, 3–5 s par groupe.

#### Plan 72 · 8:52–9:00 · 8 s
- **Dit :** Le build wasm, C ABI compris, fait tourner ce pipeline par défaut — pas un mini-minifier [README.md:392].
- **Écran :** `full default pipeline`.

#### Plan 73 · 9:00–9:08 · 8 s
- **Dit :** Rapport renvoyé : type de contenu, octets d'origine, octets compressés, octets sauvés, étapes appliquées, clés de cache, sortie [packages/wasm/README.md:53].
- **Écran :** champs `CompressionReport`.

#### Plan 74 · 9:08–9:16 · 8 s
- **Dit :** En Rust : tu crées un `LmResizer`, tu appelles `compress` avec la sortie d'outil et la tâche en cours, tu lis `report.output` [README.md:368].
- **Écran :** API Rust, karaoké = phrase dite.

#### Plan 75 · 9:16–9:24 · 8 s
- **Dit :** `image` inspecte taille et dimensions. `voice` retire des mots de remplissage de transcript. `ml-status` dit si Magika / ONNX est actif [README.md:341].
- **Écran :** trois sous-commandes.

#### Plan 76 · 9:24–9:32 · 8 s
- **Dit :** Sur une erreur de modèle ou de runtime, le chemin ONNX retombe sur la détection déterministe [README.md:122]. ONNX n'est jamais compilé dans le wasm [README.md:123].
- **Écran :** ONNX fallback · wasm sans ONNX.

#### Plan 77 · 9:32–9:40 · 8 s
- **Dit :** `lm-resizer doctor --json` fait le diagnostic local [README.md:211]. `lm-resizer wrap` lance un client derrière un timeout [README.md:354].
- **Écran :** `doctor --json` · `wrap --timeout-sec 10 codex -- --version` [README.md:353].

---

### Acte 5 — Pas encore, punchline (9:40–11:10)

#### Plan 78 · 9:40–9:48 · 8 s
- **Dit :** Ce qui n'est pas encore là, tel que le README le liste [README.md:503].
- **Écran :** titre `Not yet implemented`.

#### Plan 79 · 9:48–9:56 · 8 s
- **Dit :** Les identifiants du registre npm externe, et l'approbation de release [README.md:505].
- **Écran :** npm credentials : hors du binaire.

#### Plan 80 · 9:56–10:04 · 8 s
- **Dit :** Les filtres d'écosystème spécifiques à un projet, au-delà des intégrés et des modèles de contribution [README.md:506].
- **Écran :** built-ins oui · custom projet : pas encore.

#### Plan 81 · 10:04–10:12 · 8 s
- **Dit :** Des intégrations agent plus profondes que le rewrite PreToolUse plus la télémétrie PostToolUse — par exemple des surfaces de contexte au niveau session [README.md:508].
- **Écran :** session-level : not yet.

#### Plan 82 · 10:12–10:20 · 8 s
- **Dit :** Ensemble, Code Explorer dit où regarder. lm-resizer empêche de brûler la fenêtre après que les commandes ont parlé. La preuve brute reste récupérable par CCR [README.md:83].
- **Écran :** WHERE / COMPRESS / RECOVER.

#### Plan 83 · 10:20–10:28 · 8 s
- **Dit :** C'est l'une des trois briques que le studio utilise sur de vrais dépôts. Les deux autres : Code Buddy, l'agent, et Code Explorer, la carte [LINKEDIN-2026-09-lm-resizer.md:35].
- **Écran :** trois briques Agile Up.

#### Plan 84 · 10:28–10:36 · 8 s
- **Dit :** Site, npm, GitHub : les trois liens sont en description [LINKEDIN-2026-09-lm-resizer.md:5].
- **Écran :** les trois URLs.

#### Plan 85 · 10:36–10:44 · 8 s
- **Dit :** Si tu n'emportes qu'une phrase : un `cargo test` a fait [README.md:8] 1,23 Mo. lm-resizer en a rendu [README.md:8] 372 Ko. [README.md:8] 222 247 tokens. Rien n'est perdu.
- **Écran :** répétition du chiffre-choc.

#### Plan 86 · 10:44–10:52 · 8 s · 🎥 Lisa #5
- **Dit :** Les grandes fenêtres de contexte ne rendent pas le bruit inoffensif. Elles le rendent lourd. Garde le signal [LINKEDIN-2026-09-lm-resizer.md:21].
- **Écran :** Lisa regard caméra. Punchline.

#### Plan 87 · 10:52–11:00 · 8 s
- **Dit :** `npm install @phuetz/lm-resizer` pour le JSON. Le binaire Rust pour les logs. Lien en description.
- **Écran :** deux chemins, honnêtes.

#### Plan 88 · 11:00–11:10 · 10 s
- **Dit :** Prochaine vidéo : Code Explorer. Tes agents relisent le dépôt à chaque session. On leur donne une carte. À tout de suite.
- **Écran :** tease Code Explorer, **aucun lien GitHub**. Carton `@lisaiafr`.

---

**Durée voix estimée : 11 min 10 s.**  
**Plans : 88 (médiane ~8 s, cold open ≤ 4 s, aucun insert Lisa > 8 s).**

---

## 5) Captures à enregistrer

Dossier de dépôt : `videos-lisa-2026-09-09/captures/lm-resizer/`  
Racine produit (lecture seule) : `~/DEV/lm-resizer-master-wt/`

| # | Fichier cible | Commande / source | Dossier de travail | On doit voir |
| --- | --- | --- | --- | --- |
| C1 | `01-hero.png` | copie | `docs/lm-resizer-hero.png` | le hero [README.md:6] : un `cargo test` chiffré |
| C2 | `02-exec-git-status.mp4` | `lm-resizer exec -- git status` | `~/DEV/lm-resizer-master-wt` | sortie filtrée, pas un dump brut |
| C3 | `03-exec-json-help.mp4` | `lm-resizer exec --json -- git status` | même | JSON avec `exit_code`, `original_bytes`, `compressed_bytes` |
| C4 | `04-rewrite-git.mp4` | `lm-resizer rewrite -- git status` | même | l'équivalent `lm-resizer exec -- git status`, rien n'est lancé d'autre |
| C5 | `05-doctor.mp4` | `lm-resizer doctor --json` | même | diagnostic local |
| C6 | `06-stats.mp4` | `lm-resizer stats` | même | compteurs d'économies (historique local, pas une session) |
| C7 | `07-retrieve.mp4` | `lm-resizer exec --json -- git log -20` puis `lm-resizer retrieve <hash>` si un hash CCR apparaît | même | hash + relecture du brut |
| C8 | `08-wasm-smoke.mp4` | `node packages/wasm/smoke.mjs` | racine lm-resizer, Node [packages/wasm/README.md:8] ≥ 18 | fumigène [packages/wasm/README.md:59] 3 161 → 1 222 |
| C9 | `09-compress-json.mp4` | petit script Node : `initLmResizerWasmFromPackage` + `compressJson` sur un JSON répétitif | même | `bytes_saved` > 0, `steps_applied` |
| C10 | `10-compress-dense.mp4` | même API sur `{"a":1}` | même | `bytes_saved: 0`, pas une erreur [packages/wasm/README.md:59] |
| C11 | `11-ml-status.mp4` | `lm-resizer ml-status --json` | même | Magika inactif par défaut |
| C12 | `12-serve-health.mp4` | `lm-resizer serve --bind 127.0.0.1:8787` puis `curl -sS http://127.0.0.1:8787/health` | même | `/health` répond ; arrêter le serveur après |

Ne pas filmer un `cargo test` complet du dépôt comme « la » preuve des [README.md:8] 222 247 tokens : ce chiffre est celui du README (hero). Si on rejoue un `cargo test` à travers `exec`, afficher les octets de CETTE run, sans les relabeler 222 247.

---

## 6) Cinq chapitres YouTube

```
00:00 222 247 tokens sur un cargo test
00:20 Le bruit remplit la fenêtre
02:20 exec, le filtre, la récupération
04:50 Hooks, MCP, proxy
07:10 Le paquet npm ne filtre pas tes logs
```
