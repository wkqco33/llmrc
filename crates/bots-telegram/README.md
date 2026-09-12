# llmrc-bots-telegram

Telegram event mapping for [`llmrc`](https://github.com/wkqco33/llmrc) bots.

`TelegramAdapter` converts Telegram chat identifiers, authors, and message text
into `llmrc-bots-core::BotEvent` values (platform tag `telegram`) and extracts
response text for sending back. This is a thin mapper: your application still
drives the Telegram update loop with [teloxide](https://crates.io/crates/teloxide).

## Features

| Feature | Default | Description |
| --- | --- | --- |
| `teloxide` | no | Reserved for a future teloxide integration; adds no dependencies today |

Most applications should depend on the [`llmrc`](https://crates.io/crates/llmrc)
facade with the `telegram` feature instead of using this crate directly.

## Install

```toml
[dependencies]
llmrc-bots-telegram = "0.1"
```

## Usage

```rust
use llmrc_bots_telegram::TelegramAdapter;

let event = TelegramAdapter::event("987654", Some("tg_user".into()), "hello");
assert_eq!(event.key.platform, "telegram");
```

## Documentation

- [User guide](https://github.com/wkqco33/llmrc/blob/master/GUIDE.md)
- [API reference](https://docs.rs/llmrc-bots-telegram)

## License

MIT
