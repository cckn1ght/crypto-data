# Repository Guidelines

## Project Structure & Module Organization
This repository is a Rust CLI tool for downloading Binance market data.

- `src/main.rs`: CLI entrypoint (`list-symbols`, `get-klines`).
- `src/utils.rs`: request/query helpers, CSV writing, datetime parsing, download flow.
- `src/constants.rs`: API base URLs, enums, shared constants (for example `KLINE_LIMIT`).
- `src/types.rs`: API/data structs and shared types.
- `images/`: README assets and screenshots.
- `target/`: build artifacts (generated, do not edit manually).

Keep new logic inside `src/` and split by responsibility instead of growing `main.rs`.

## Build, Test, and Development Commands
- `cargo build`: compile debug binary.
- `cargo run -- --help`: run CLI and inspect top-level options.
- `cargo run -- get-klines --help`: inspect kline download options.
- `cargo test`: run unit tests.
- `cargo fmt`: format Rust code.
- `cargo clippy --all-targets --all-features -D warnings`: strict linting before PR.

## Coding Style & Naming Conventions
- Follow Rust 2021 idioms and keep formatting `rustfmt`-clean.
- Use `snake_case` for functions/variables/modules and `PascalCase` for structs/enums.
- Prefer explicit types for public interfaces and clear error propagation (`Result`) over silent fallbacks.
- Keep constants centralized in `constants.rs`; avoid magic numbers in business logic.

## Testing Guidelines
- Use Rust’s built-in test framework (`#[cfg(test)]`, `#[test]`).
- Place small unit tests close to the module they verify (current pattern in `src/utils.rs`).
- Name tests by behavior, for example `parse_date_time_accepts_unix_ms`.
- Run `cargo test` locally before submitting; add tests for new parsing, query, or CSV behaviors.

## Commit & Pull Request Guidelines
- Follow observed Conventional Commit style: `feat: ...`, `fix: ...`, `chore: ...`.
- Keep commits focused and single-purpose.
- PRs should include: what changed and why, how it was validated (commands run), and sample CLI invocation/output when behavior changes.
- Update `README.md` when command flags, defaults, or usage flow change.

## Security & Configuration Tips
- Never hardcode secrets or proxy credentials.
- Use environment variables for proxy/network config (`HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`) when needed.
