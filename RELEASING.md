# Releasing

1. Update the version in `[workspace.package]` in `Cargo.toml`.
2. Update `CHANGELOG.md`.
3. Run the release checks:

   ```sh
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets --locked -- -D warnings
   cargo test --workspace --locked
   RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
   cargo package -p stmob --locked
   cargo package -p stmob-cli --locked --config 'patch.crates-io.stmob.path="stmob"'
   cargo publish --dry-run -p stmob
   cargo publish --dry-run -p stmob-cli --config 'patch.crates-io.stmob.path="stmob"'
   ```

4. Publish the library first:

   ```sh
   cargo publish -p stmob --locked
   ```

5. Wait until the new version is available on crates.io, then publish the CLI:

   ```sh
   cargo publish -p stmob-cli
   ```

   The CLI is published without `--locked` because its `stmob` dependency is
   resolved from crates.io rather than the local path.

6. Tag the release:

   ```sh
   git tag v0.1.0
   git push origin v0.1.0
   ```
