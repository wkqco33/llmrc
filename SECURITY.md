# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

---

## Reporting a Vulnerability

The `llmrc` team takes the security of our runtime and its users seriously. If you discover a security vulnerability, please report it responsibly rather than opening a public issue.

### How to Report

Please submit vulnerability reports via:
- **Email**: `security@llmrc.dev` (or open a private security advisory on GitHub)
- **Information to include**:
  - Description of the vulnerability and affected components / crates
  - Step-by-step instructions or proof-of-concept code to reproduce the issue
  - Impact assessment (e.g., secret disclosure, credential leakage, unbounded denial of service)
  - Any suggested remediations or mitigations

### Response Timeline

- **Acknowledgment**: Within 48 hours of receipt
- **Assessment & Triage**: Within 5 business days
- **Fix & Advisory Release**: Coordinated with the reporter before public disclosure

---

## Security Best Practices in llmrc

1. **Credential Handling**:
   - Always wrap API keys and sensitive tokens in `llmrc_core::Secret`.
   - The `Secret` type intentionally redacts contents in its `Debug` implementation (`Secret(REDACTED)`).
   - Never implement `Display` for `Secret`. Access underlying strings only via explicit `.expose()` at the network call boundary.
2. **Denial-of-Service Defense**:
   - `llmrc_agent::Agent` enforces explicit limits: `max_turns`, `max_tool_calls`, `max_concurrency`, `deadline`, and `tool_timeout`.
   - `llmrc_bots_core::split_message` enforces safe UTF-8 slicing and bounded message sizes to prevent memory exhaustion.
3. **No Prompt / PII Logging**:
   - Token counters and telemetry implementations must never log or retain prompt text or completion contents in debug outputs.
