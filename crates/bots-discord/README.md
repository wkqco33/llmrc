# llmrc-bots-discord

Discord event mapping for [`llmrc`](https://github.com/wkqco33/llmrc) bots.

`DiscordAdapter` converts Discord channel identifiers, authors, and message text
into `llmrc-bots-core::BotEvent` values (platform tag `discord`) and extracts
response text for sending back. This is a thin mapper: your application still
drives the Discord gateway with [Serenity](https://crates.io/crates/serenity).

## Features

| Feature | Default | Description |
| --- | --- | --- |
| `serenity` | no | Reserved for a future Serenity integration; adds no dependencies today |

Most applications should depend on the [`llmrc`](https://crates.io/crates/llmrc)
facade with the `discord` feature instead of using this crate directly.

## Install

```toml
[dependencies]
llmrc-bots-discord = "0.1"
```

## Usage

```rust
use llmrc_bots_discord::DiscordAdapter;

let event = DiscordAdapter::event("channel-123", Some("alice".into()), "hello");
assert_eq!(event.key.platform, "discord");
```

## Documentation

- [User guide](https://github.com/wkqco33/llmrc/blob/master/GUIDE.md)
- [API reference](https://docs.rs/llmrc-bots-discord)

## License

MIT
