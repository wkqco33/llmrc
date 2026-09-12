# llmrc-agent

A bounded, cancellation-aware, tool-using agent loop for the
[`llmrc`](https://github.com/wkqco33/llmrc) LLM runtime.

The `Agent` runs a reason → tool → observe loop with hard limits on turns, tool
calls, concurrency, wall-clock time, and history length. It ships the `Tool`
trait, a `ToolRegistry` with schema validation, and a pluggable `SessionStore`.

Most applications should depend on the [`llmrc`](https://crates.io/crates/llmrc)
facade instead of using this crate directly.

## Install

```toml
[dependencies]
llmrc-agent = "0.1"
```

## Documentation

- [User guide](https://github.com/wkqco33/llmrc/blob/master/GUIDE.md)
- [API reference](https://docs.rs/llmrc-agent)

## License

MIT
