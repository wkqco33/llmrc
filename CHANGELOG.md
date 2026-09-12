# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Standard `CHANGELOG.md` tracking all release milestones and unreleased improvements.
- Explicit `documentation` URLs in workspace and crate Cargo manifests.
- Dedicated `README.md` in all 10 workspace subcrates for improved crates.io discovery and documentation.
- Executable doctests (`cargo test --doc`) across `llmrc`, `llmrc-core`, `llmrc-runtime`, `llmrc-agent`, and `llmrc-bots-core`.

## [0.1.0] - 2026-09-12

### Added
- Initial release of `llmrc`, a predictable, modular LLM runtime facade for Rust.
- Decoupled workspace architecture:
  - `llmrc-core`: Domain models (`Message`, `ChatRequest`, `ChatResponse`, `ToolDefinition`), `Secret` redaction, provider traits (`ChatProvider`, `EmbeddingProvider`), and standardized `LlmError`.
  - `llmrc-runtime`: Bounded `RetryPolicy` with jitter (`Full`, `Equal`), token accounting primitives, and safe stream retry semantics.
  - `llmrc-agent`: Bounded `Agent` execution loop with turn limits, tool call caps, semaphore-based concurrency control, and history trimming.
  - `llmrc-mcp`: Model Context Protocol (MCP) bridge with stdio and streamable HTTP transports.
  - `llmrc-bots-core`: Platform-neutral `BotHandler` with per-conversation turn serialization and UTF-8 safe message chunking.
  - Platform adapters: `llmrc-bots-discord`, `llmrc-bots-telegram`, `llmrc-bots-slack`.
  - Provider adapters: `llmrc-openai` (OpenAI and Azure OpenAI), `llmrc-ollama` (local and remote Ollama).
- Comprehensive executable examples (`examples/01_basic_agent.rs`, `02_custom_tools.rs`, `03_bot_conversation.rs`, `04_streaming_and_retry.rs`).
- English and Korean documentation (`README.md`, `README.ko.md`, `GUIDE.md`, `GUIDE.ko.md`, `RELEASING.md`, `RELEASING.ko.md`).
- Multi-OS CI workflow and tag-driven automated crates.io publishing pipeline.
