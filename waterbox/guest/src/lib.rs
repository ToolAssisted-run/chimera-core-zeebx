//! The Zeebx core: the functions Chimera's engine calls. A first skeleton,
//! to find out what of Zeebx builds for the sandbox.
#![allow(non_snake_case)]

#[no_mangle]
pub extern "C" fn Init() -> i32 {
    // names the session type so that the whole emulator is linked
    let _ = std::mem::size_of::<zeebx::session::Session>();
    0
}
