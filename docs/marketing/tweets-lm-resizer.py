import re, json
# Comptage X (twitter-text v3) : URL = 23 ; emoji = 2 ; plages U+0000-10FF, U+2000-200D, U+2010-201F, U+2032-2037 = 1 ; reste = 2
def xlen(t):
    t = re.sub(r'https?://\S+', 'x'*23, t)
    n = 0
    for ch in t:
        o = ord(ch)
        if o in (0xFE0F, 0x200D): continue
        n += 1 if (o <= 0x10FF or 0x2000 <= o <= 0x200D or 0x2010 <= o <= 0x201F or 0x2032 <= o <= 0x2037) else 2
    return n
T = {}
T['1/6'] = """Un cargo test : 323 lignes de sortie.
Ce que mon agent de code lit avec lm-resizer : 19 lignes. Le test en échec, l'assertion, le compteur final.
La sortie brute reste sur le disque, récupérable.

Un binaire Rust pour Claude Code, Codex et les agents MCP 🧵"""
T['2/6'] = """Le principe : lm-resizer se place entre la commande et le modèle.

lm-resizer exec -- cargo test

Filtres par famille (git, cargo, rg, vitest/jest, Terraform, kubectl...), puis compression. Code de sortie conservé.
Avec --stream, vous voyez défiler la sortie en direct."""
T['3/6'] = """Sans changer vos habitudes :

lm-resizer init-native-hooks --client all

Claude Code / Codex exécutent la version filtrée des commandes Bash connues. Commande inconnue = sortie brute, jamais bloquée.
Ou en serveur MCP : lm-resizer install --client claude"""
T['4/6'] = """Au-delà du terminal :
- proxy local devant une API compatible OpenAI/Anthropic (lm-resizer serve)
- compression orientée requête : s'il faut couper, on garde ce qui touche votre question
- blocs retirés stockés en SQLite local : lm-resizer retrieve <hash>"""
T['5/6'] = """Les chiffres, dits précisément.
Démo ci-dessus : 9 026 -> 660 octets.
Cumul d'une session de 398 commandes passées par lm-resizer (pas un seul cargo test) : 1,23 Mo -> 372 Ko, 222 247 tokens estimés économisés.

L'idée n'est pas neuve : RTK et Headroom sont les références."""
T['6/6'] = """Installer (Rust stable récent + compilateur C++) :
cargo install --locked --git https://github.com/phuetz/lm-resizer

Apache-2.0, 100 % local, pas de télémétrie.
Repo 👉 https://github.com/phuetz/lm-resizer

Projet jeune : dites-moi quelle commande inonde encore votre contexte."""
T['alt-A'] = """Votre agent de code paie des tokens pour lire "test ... ok" 300 fois.

lm-resizer ne lui montre que ce qui compte : l'échec, l'assertion, le résumé. Le brut reste récupérable en local.

Démo réelle en 40 s 👇"""
T['alt-B'] = """Une fenêtre de 1M de tokens ne rend pas le bruit gratuit : vous le payez en coût, en latence et en attention du modèle.

lm-resizer filtre la sortie des commandes avant qu'elle n'atteigne Claude Code ou Codex. Rust, Apache-2.0 👇"""
bad = ['open'+' source','open'+'-source','open'+'source','•','→','…','@phuetz/']
for k, v in T.items():
    assert not any(b in v.lower() for b in bad), k
    print(k, xlen(v))
json.dump({k: {'text': v, 'x_len': xlen(v)} for k, v in T.items()}, open('tweets.json', 'w'), ensure_ascii=False, indent=1)
