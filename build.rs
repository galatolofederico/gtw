// Windows gives a program's main thread a 1 MB stack (Linux and macOS give
// 8 MB). The game runs its event loop and builds most of its GPU pipelines on
// the main thread, and on DirectX 12 each shader goes through naga's HLSL
// writer and then DXC, in process and on the calling thread. The flight
// scene's shaders overflowed 1 MB there (exit 0xC00000FD, STATUS_STACK_OVERFLOW,
// right after the aircraft was built), so reserve 16 MB for the main thread.
// This only reserves address space; pages are committed as they are used.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    const STACK: u32 = 16 << 20;
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
            Ok("msvc") => println!("cargo:rustc-link-arg-bins=/STACK:{STACK}"),
            Ok("gnu") => println!("cargo:rustc-link-arg-bins=-Wl,--stack,{STACK}"),
            _ => {}
        }
    }
}
