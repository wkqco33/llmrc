# llmrc-bots-slack

Thin, platform-neutral Slack mapping for [`llmrc`](https://crates.io/crates/llmrc) bots.

## Overview

`llmrc-bots-slack` maps Slack events into platform-neutral `BotEvent` representations and converts `BotResponse` into Slack-compliant payloads.

## Usage

Via top-level [`llmrc`](https://crates.io/crates/llmrc):

```toml
[dependencies]
llmrc = { version = "0.1", features = ["slack"] }
```

Direct dependency:

```toml
[dependencies]
llmrc-bots-slack = "0.1"
```

## Documentation

Full documentation is available on [docs.rs](https://docs.rs/llmrc-bots-slack).
