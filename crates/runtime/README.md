# llmrc-runtime

Retry policies and token accounting for the
[`llmrc`](https://github.com/wkqco33/llmrc) LLM runtime.

Provides `RetryPolicy` (bounded exponential backoff with jitter, shared
`RetryBudget`, and `Retry-After` honoring), plus pluggable `TokenCounter`
implementations and `TokenAccounting` with explicit
`Exact` / `ProviderReported` / `Estimated` provenance.

Most applications should depend on the [`llmrc`](https://crates.io/crates/llmrc)
facade instead of using this crate directly.

## Install

```toml
[dependencies]
llmrc-runtime = "0.1"
```

## Documentation

- [User guide](https://github.com/wkqco33/llmrc/blob/master/GUIDE.md)
- [API reference](https://docs.rs/llmrc-runtime)

## License

MIT
