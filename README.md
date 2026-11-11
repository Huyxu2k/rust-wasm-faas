## 1. Development Environment Install
- ##### 1. Install Wasmtime runtime (CLI)
```
    winget install BytecodeAlliance.Wasmtime 
    (curl https://wasmtime.dev -sSf | bash)
```
- ##### 2. Cài đặt cargo-component (Mở rộng của Cargo cho Wasm Component Model)
```
    cargo install cargo-component --locked
```
- ##### 3. Thêm mục tiêu biên dịch Wasm vào Rust toolchain của bạn
```
    rustup target add wasm32-wasip2
    cargo install wasmtime-cli 
```
- ##### 4. Run test tenant service
For example
```
wasmtime serve --addr 127.0.0.1:3000 -Scli -Shttp target/wasm32-wasip2/release/yourname.wasm
```
- ##### 5. Upload file wasm 
For example
```
curl --location 'http://127.0.0.1:8080/tenants/upload' \
--form 'file=@"d:\\HUY\\RUST\\rust-wasm-faas\\target\\wasm32-wasip2\\release\\service-test1.wasm"'
```
- ##### 6. Call with tenant_id
For example
```
curl --location 'http://127.0.0.1:8080/fn/4bef388b-91a5-470e-b457-d09e17212b60'
```