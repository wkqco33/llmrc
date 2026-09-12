# llmrc-bots-slack

Slack event mapping for [`llmrc`](https://github.com/wkqco33/llmrc) bots.

`SlackAdapter` converts Slack channel identifiers, authors, and message text
into `llmrc-bots-core::BotEvent` values (platform tag `slack`) and extracts
response text for sending back. This is a thin mapper: your application still
drives Slack with
[slack-morphism](https://crates.io/crates/slack-morphism).

## Features

| Feature | Default | Description |
| --- | --- | --- |
| `slack-morphism` | no | Reserved for a future slack-morphism integration; adds no dependencies today |

Most applications should depend on the [`llmrc`](https://crates.io/crates/llmrc)
facade with the `slack` feature instead of using this crate directly.

## Install

```toml
[dependencies]
llmrc-bots-slack = "0.1"
```

## Usage

```rust
use llmrc_bots_slack::SlackAdapter;

let event = SlackAdapter::event("C12345", Some("U67890".into()), "hello");
assert_eq!(event.key.platform, "slack");
```

## Documentation

- [User guide](https://github.com/wkqco33/llmrc/blob/master/GUIDE.md)
- [API reference](https://docs.rs/llmrc-bots-slack)

## License

MIT
