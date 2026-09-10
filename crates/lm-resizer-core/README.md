# lm-resizer-core

Core library of [lm-resizer](https://github.com/phuetz/lm-resizer): the
compression transforms that keep noisy command output (tests, builds, package
managers, diffs, logs) out of an LLM agent's context window, and the CCR store
that keeps every raw output recoverable.

Use the `lm-resizer` crate for the CLI, the MCP server and the agent hooks;
depend on `lm-resizer-core` to embed the transforms in your own tool.

```toml
[dependencies]
lm-resizer-core = "0.2"
```

Licensed under Apache-2.0.
