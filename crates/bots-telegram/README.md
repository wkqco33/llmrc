# llmrc-bots-telegram

Thin, platform-neutral Telegram mapping for [`llmrc`](https://crates.io/crates/llmrc) bots.

## Overview

`llmrc-bots-telegram` maps Telegram messages into platform-neutral `BotEvent` representations and converts `BotResponse` into Telegram-compliant payloads.

## Usage

Via top-level [`llmrc`](https://crates.io/crates/llmrc):

```toml
[dependencies]
llmrc = { version = "0.1", features = ["telegram"] }
```

Direct dependency:

```toml
[dependencies]
llmrc-bots-telegram = "0.1"
```

## Documentation

Full documentation is available on [docs.rs](https://docs.rs/llmrc-bots-telegram).
