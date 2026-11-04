# tcg-engine
A high-performance Trading Card Game runtime.

## Development

The toolchain is pinned in `rust-toolchain.toml` (Rust 1.95.0 with rustfmt, clippy and the `wasm32-unknown-unknown` target); rustup installs it on the first `cargo` call. Before opening a PR, run the gate:

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p tcg-engine --target wasm32-unknown-unknown
```

The determinism rules and how they are enforced are in `CLAUDE.md`.
