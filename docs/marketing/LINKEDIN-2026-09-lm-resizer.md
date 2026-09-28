# LinkedIn — lm-resizer 0.2.1 (brouillons, à publier par Patrice / page Agile Up)

Le chiffre ancien « 398 commandes, 222 247 jetons » a été retiré faute de sortie brute et de méthode versionnées. Pour les mesures publiables, utiliser uniquement `bench/README.md`, `bench/resultats.json` et le rapport du banc, avec leurs limites explicites. Ne pas présenter une économie de sortie comme une économie facturée.

Visuel conseillé : `docs/lm-resizer-hero.png`. Liens : https://www.npmjs.com/package/@phuetz/lm-resizer · https://github.com/phuetz/lm-resizer · https://phuetz.github.io/lm-resizer/

---

## Version profil (« je »)

Un agent de code passe une part énorme de sa fenêtre de contexte à lire du bruit : sorties de tests, logs de npm, diffs, listings. Ça coûte des tokens, ça ralentit, et ça cache l'erreur qui compte.

lm-resizer se met entre la commande et le modèle pour réduire le bruit. Le banc reproductible compare 22 sorties synthétiques à RTK et Headroom, avec des oracles de conservation ; consulter ses résultats avant de citer un gain. La sortie brute reste récupérable localement.

C'est en Rust, ça marche en enveloppe de commande, en proxy HTTP, en serveur MCP, et en hook pour Claude Code et Codex. Compression « consciente de la question » : quand il faut couper, il garde ce qui répond à ce que vous demandez.

Nouveau aujourd'hui : le module WebAssembly est sur npm avec une vraie fiche et un chargeur intégré, trois lignes pour compresser un JSON depuis Node.

npm install @phuetz/lm-resizer

Les grandes fenêtres de contexte ne rendent pas le bruit gratuit. Elles le rendent cher trois fois : au prix, à la latence, et à l'attention du modèle.

#IA #LLM #Rust #DeveloperTools #ClaudeCode #Codex #MCP

---

## Version page Agile Up (« nous »)

Plus la fenêtre de contexte est grande, plus le bruit coûte cher.

lm-resizer, notre outil open source en Rust, filtre la sortie des commandes avant qu'elle n'atteigne l'agent de code. Un banc reproductible compare 22 sorties synthétiques à RTK et Headroom et contrôle la présence des faits attendus.

Il s'installe en hook dans Claude Code et Codex, en proxy devant n'importe quelle API compatible OpenAI ou Anthropic, ou en serveur MCP. Le module WebAssembly vient d'arriver sur npm.

C'est l'un des trois briques que nous utilisons chaque jour pour faire travailler des agents sur de vrais dépôts. Les deux autres : Code Buddy, l'agent, et Code Explorer, la carte du code.

github.com/phuetz/lm-resizer

#IA #Rust #OpenSource #AgileUp #AgentsIA

---

## English (short)

Your coding agent burns most of its context on noise: test output, package manager logs, diffs. lm-resizer sits between the command and the model and keeps the signal.

The reproducible benchmark compares 22 synthetic command outputs with RTK and Headroom, checking declared facts in each result. Full raw output remains locally recoverable.

Rust. CLI wrapper, HTTP proxy, MCP server, Claude Code / Codex hooks. Query-aware compression. The WebAssembly module is now on npm with a bundled loader.

npm install @phuetz/lm-resizer

#AI #LLM #Rust #DevTools

---

Checklist avant de poster : l'image hero est lisible sur mobile ; les chiffres cités viennent du banc versionné ; ne pas promettre la compression de texte dans le paquet npm (elle est dans le binaire).
