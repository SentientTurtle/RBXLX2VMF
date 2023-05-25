cargo build --target wasm32-unknown-unknown --release
wasm-bindgen --target no-modules --no-typescript --out-dir . "./target/wasm32-unknown-unknown/release/rbxlx2vmf_web.wasm"
mv "./rbxlx2vmf_web_bg.wasm" "./html/rbxlx2vmf_web_bg.wasm"
mv "./rbxlx2vmf_web.js" "./html/rbxlx2vmf_web.js"