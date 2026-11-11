## 1. Development Environment Install
- 1. Install Wasmtime runtime (CLI)
```
    winget install BytecodeAlliance.Wasmtime 
    (curl https://wasmtime.dev -sSf | bash)
```
- 2. Cài đặt cargo-component (Mở rộng của Cargo cho Wasm Component Model)
```
    cargo install cargo-component --locked
```
- 3. Thêm mục tiêu biên dịch Wasm vào Rust toolchain của bạn
```
    rustup target add wasm32-wasip2
    cargo install wasmtime-cli 
```
- 4. Run tenant service
```
wasmtime serve --addr 127.0.0.1:3000 -Scli -Shttp target/wasm32-wasip2/release/yourname.wasm
```