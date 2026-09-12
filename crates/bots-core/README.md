# llmrc-bots-core

Platform-neutral chat bot runtime for [`llmrc`](https://github.com/wkqco33/llmrc).

`BotHandler` runs one `llmrc-agent` `Agent` per conversation. Turns are locked
per `ConversationKey`, so a busy channel cannot corrupt its own session while
unrelated channels run in parallel. Long replies are split on UTF-8 boundaries
with `split_message`, and the `PlatformAdapter` trait maps native events in and
out.

Most applications should depend on the [`llmrc`](https://crates.io/crates/llmrc)
facade with the `bots` feature instead of using this crate directly.

## Install

```toml
[dependencies]
llmrc-bots-core = "0.1"
```

## Documentation

- [User guide](https://github.com/wkqco33/llmrc/blob/master/GUIDE.md)
- [API reference](https://docs.rs/llmrc-bots-core)

## License

MIT
