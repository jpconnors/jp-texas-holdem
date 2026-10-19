//! JP's Texas Hold'em engine: rules, hand evaluation and AI opponents.
//!
//! Built natively for tests (`cargo test --release`) and to `wasm32-unknown-unknown` for the browser.
//! The browser talks to it through three exports that pass JSON in linear memory:
//!
//!   jp_alloc(len) -> ptr        reserve `len` bytes for a UTF-8 JSON request
//!   jp_call(ptr, len) -> len    run the request (frees the input), returns the response length
//!   jp_out_ptr() -> ptr         where the response bytes are
//!
//! See `api.rs` for the command set and `js/wasm-game.js` for the JavaScript side.

pub mod ai;
pub mod api;
pub mod cards;
pub mod eval;
pub mod game;
pub mod rng;

use std::cell::RefCell;

thread_local! {
    static API: RefCell<api::Api> = RefCell::new(api::Api::default());
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Allocates a zeroed buffer of exactly `len` bytes for the caller to fill.
#[no_mangle]
pub extern "C" fn jp_alloc(len: usize) -> *mut u8 {
    Box::into_raw(vec![0u8; len].into_boxed_slice()) as *mut u8
}

/// Runs one JSON command. Takes ownership of (and frees) the buffer from `jp_alloc`.
///
/// # Safety
/// `ptr`/`len` must come from a single `jp_alloc(len)` call and not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn jp_call(ptr: *mut u8, len: usize) -> usize {
    let input: Box<[u8]> = Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len));
    let out = API.with(|a| a.borrow_mut().handle_bytes(&input));
    OUT.with(|o| {
        *o.borrow_mut() = out;
        o.borrow().len()
    })
}

/// Pointer to the bytes of the last response.
#[no_mangle]
pub extern "C" fn jp_out_ptr() -> *const u8 {
    OUT.with(|o| o.borrow().as_ptr())
}
