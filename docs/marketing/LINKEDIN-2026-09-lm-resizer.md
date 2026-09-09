# LinkedIn — lm-resizer 0.2.1 (brouillons, à publier par Patrice / page Agile Up)

Faits vérifiés le 09/09/2026 : README `master` (hero : un `cargo test` à travers lm-resizer = 398 commandes, 1,23 Mo → 372 Ko, 222 247 tokens économisés, sortie complète récupérable) ; fumigène du paquet npm : 3 161 → 1 222 octets ; publié sur npm par trusted publishing avec provenance ; validé devant Mistral, Ollama, DeepSeek, OpenRouter, xAI (23/06). Ne pas ajouter de chiffre absent d'ici.

Visuel conseillé : `docs/lm-resizer-hero.png`. Liens : https://www.npmjs.com/package/@phuetz/lm-resizer · https://github.com/phuetz/lm-resizer · https://phuetz.github.io/lm-resizer/

---

## Version profil (« je »)

Un agent de code passe une part énorme de sa fenêtre de contexte à lire du bruit : sorties de tests, logs de npm, diffs, listings. Ça coûte des tokens, ça ralentit, et ça cache l'erreur qui compte.

lm-resizer se met entre la commande et le modèle et ne garde que le signal. Une seule exécution de `cargo test` à travers lui : 398 commandes, 1,23 Mo → 372 Ko, 222 247 tokens économisés. Les erreurs, les chemins de fichiers et le résumé restent visibles, et la sortie complète reste récupérable si l'agent en a besoin.

C'est en Rust, ça marche en enveloppe de commande, en proxy HTTP, en serveur MCP, et en hook pour Claude Code et Codex. Compression « consciente de la question » : quand il faut couper, il garde ce qui répond à ce que vous demandez.

Nouveau aujourd'hui : le module WebAssembly est sur npm avec une vraie fiche et un chargeur intégré, trois lignes pour compresser un JSON depuis Node.

npm install @phuetz/lm-resizer

Les grandes fenêtres de contexte ne rendent pas le bruit gratuit. Elles le rendent cher trois fois : au prix, à la latence, et à l'attention du modèle.

#IA #LLM #Rust #DeveloperTools #ClaudeCode #Codex #MCP

---

## Version page Agile Up (« nous »)

Plus la fenêtre de contexte est grande, plus le bruit coûte cher.

lm-resizer, notre outil open source en Rust, filtre la sortie des commandes avant qu'elle n'atteigne l'agent de code : un `cargo test` complet passe de 1,23 Mo à 372 Ko, 222 247 tokens économisés, sans perdre une erreur ni un chemin de fichier.

Il s'installe en hook dans Claude Code et Codex, en proxy devant n'importe quelle API compatible OpenAI ou Anthropic, ou en serveur MCP. Le module WebAssembly vient d'arriver sur npm.

C'est l'un des trois briques que nous utilisons chaque jour pour faire travailler des agents sur de vrais dépôts. Les deux autres : Code Buddy, l'agent, et Code Explorer, la carte du code.

github.com/phuetz/lm-resizer

#IA #Rust #OpenSource #AgileUp #AgentsIA

---

## English (short)

Your coding agent burns most of its context on noise: test output, package manager logs, diffs. lm-resizer sits between the command and the model and keeps the signal.

One `cargo test` through it: 398 commands, 1.23 MB → 372 KB, 222,247 tokens saved. Errors, file paths and summaries stay; the full output stays recoverable.

Rust. CLI wrapper, HTTP proxy, MCP server, Claude Code / Codex hooks. Query-aware compression. The WebAssembly module is now on npm with a bundled loader.

npm install @phuetz/lm-resizer

#AI #LLM #Rust #DevTools

---

Checklist avant de poster : l'image hero est lisible sur mobile ; le chiffre 222 247 correspond au README ; ne pas promettre la compression de texte dans le paquet npm (elle est dans le binaire).
