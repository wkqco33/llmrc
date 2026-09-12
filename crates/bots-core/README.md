# llmrc-bots-core

Platform-neutral conversation and bot runtime for [`llmrc`](https://crates.io/crates/llmrc).

## Overview

`llmrc-bots-core` orchestrates LLM bot conversations across chat platforms:

- **Per-Conversation Serialization**: Serializes turns within the same conversation to prevent state corruption while allowing disjoint conversations to run concurrently.
- **UTF-8 Safe Splitting**: `split_message` breaks long LLM outputs into platform-bounded chunks (e.g. Discord 2000 chars, Slack 4000 chars) respecting character/grapheme boundaries.
- **Platform Abstraction**: `PlatformAdapter`, `BotEvent`, `BotResponse`, and `BotHandler`.

## Usage

Via top-level [`llmrc`](https://crates.io/crates/llmrc):

```toml
[dependencies]
llmrc = { version = "0.1", features = ["bots"] }
```

Direct dependency:

```toml
[dependencies]
llmrc-bots-core = "0.1"
```

## Documentation

Full documentation is available on [docs.rs](https://docs.rs/llmrc-bots-core).
