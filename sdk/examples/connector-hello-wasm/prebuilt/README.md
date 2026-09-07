# Prebuilt `connector-hello-wasm` component

`connector_hello_wasm.wasm` is the `wasm32-wasip2` build of the example in
`../src/lib.rs`, checked in so the sandbox's positive test
(`register_hello_component_from_sdk_world` in
`crates/springtale-connector/src/wasm/tier/cache.rs`) can prove a real
component built against `sdk/connector-sdk/wit/connector.wit` links
against the host's WASI Preview 2 linker — without needing a wasm
toolchain, Python, or Node at test time.

Regenerate after changing `../src/lib.rs` or the WIT world:

```sh
rustup target add wasm32-wasip2
cd sdk/examples/connector-hello-wasm
cargo build --release --target wasm32-wasip2
cp target/wasm32-wasip2/release/connector_hello_wasm.wasm prebuilt/
```

The `wasm32-wasip2` target emits a component directly — there is no
`wasm-tools component new` step. CI rebuilds the example on every push
(`.github/workflows/ci.yml`, job `wasm-sdk`), so a source change that
stops compiling is caught even though the artefact here is not
byte-compared (rustc output is not reproducible across toolchains).
