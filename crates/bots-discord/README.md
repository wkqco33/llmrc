# llmrc-bots-discord

Thin, platform-neutral Discord mapping for [`llmrc`](https://crates.io/crates/llmrc) bots.

## Overview

`llmrc-bots-discord` maps Discord messages and thread events into platform-neutral `BotEvent` representations and converts `BotResponse` into Discord-compliant payloads.

## Usage

Via top-level [`llmrc`](https://crates.io/crates/llmrc):

```toml
[dependencies]
llmrc = { version = "0.1", features = ["discord"] }
```

Direct dependency:

```toml
[dependencies]
llmrc-bots-discord = "0.1"
```

## Documentation

Full documentation is available on [docs.rs](https://docs.rs/llmrc-bots-discord).
