// Hint cargo to rerun when frontend assets change so Tauri's bundler picks up
// fresh JS/CSS without a manual `cargo clean`.
fn main() {
    println!("cargo::rerun-if-changed=../../dashboard/static/");
    println!("cargo::rerun-if-changed=../../dashboard/index.html");
}
