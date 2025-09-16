# Basis Tracker Development Guide

## Build & Test Commands
- `cargo build` - Build all crates
- `cargo check` - Type check without building
- `cargo test` - Run all tests
- `cargo test -p <crate_name>` - Run tests for specific crate
- `cargo test --test <test_name>` - Run specific test
- `cargo clippy` - Lint with Clippy
- `cargo fmt` - Format code

## Code Style Guidelines
- **Rust 2021 edition** with standard formatting
- **Imports**: Group std, external, internal crates with blank lines
- **Naming**: snake_case for variables/functions, PascalCase for types
- **Error handling**: Use `Result` and `?` operator, avoid unwrap()
- **Documentation**: Use /// doc comments for public items
- **Dependencies**: Use workspace dependencies when possible

## Project Structure
- Multi-crate workspace under `crates/` directory
- Each crate has specific purpose (app, server, store, cli, offchain)
- Shared dependencies in workspace Cargo.toml

## Testing
- Unit tests in `src/` files with `#[cfg(test)]` mod
- Integration tests in `tests/` directory
- Use `#[test]` attribute for test functions

## Common Patterns
- Async/await with Tokio runtime
- Tracing for logging
- Serde for serialization
- Ergo blockchain integration