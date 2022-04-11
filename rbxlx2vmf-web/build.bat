cargo build --target wasm32-unknown-unknown --release
wasm-bindgen --target web --no-typescript --out-dir . "./target/wasm32-unknown-unknown/release/rbxlx2vmf_web.wasm"
wasm-gc "./rbxlx2vmf_web_bg.wasm"
move "./rbxlx2vmf_web_bg.wasm" "./html/rbxlx2vmf_web_bg.wasm"
move "./rbxlx2vmf_web.js" "./html/rbxlx2vmf_web.js"