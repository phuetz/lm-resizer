# Grok skill (LM Resizer)

https://github.com/phuetz/lm-resizer

```bash
./scripts/install-grok-skill.sh --target grok
./scripts/install-grok-skill.sh --target codex
```

Idempotent; skips the whole copy if any dest file differs (missing-file repair is then skipped too). `--force` backups dest. Grok discovery: `grok inspect --json`. No `grok` MCP client. Claude/Buddy not installed by this script.
