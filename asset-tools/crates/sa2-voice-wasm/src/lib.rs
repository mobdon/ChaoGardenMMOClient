//! `ahx.wasm`: decodes one AHX voice entry to a float WAVE with the sa2 crate's
//! own MPEG layer II decoder. JS calls `alloc`, copies the entry in, calls
//! `decode`, reads `out_ptr`/`out_len`, then `free`s both buffers.
//! Build: `cargo build --release --target wasm32-unknown-unknown`.

use std::cell::RefCell;

/// Largest WAVE we hand back (a voice line is a few seconds).
const MAX_WAV: usize = 64 << 20;

thread_local! {
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Decodes `bytes`; the WAVE (or nothing on error) is kept for `out_ptr`/`out_len`.
pub fn decode_to_out(bytes: &[u8]) -> bool {
    let wav = sa2_asset_tools::audio::decode_ahx_wav(bytes, MAX_WAV);
    let ok = wav.is_ok();
    OUT.with(|o| *o.borrow_mut() = wav.unwrap_or_default());
    ok
}

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut v = vec![0u8; len.max(1)];
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `ptr`/`len` must come from one `alloc` call.
#[no_mangle]
pub unsafe extern "C" fn free(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, len.max(1), len.max(1)));
}

/// # Safety
/// `ptr`/`len` must be an `alloc`ed buffer holding the AHX bytes.
#[no_mangle]
pub unsafe extern "C" fn decode(ptr: *const u8, len: usize) -> i32 {
    decode_to_out(std::slice::from_raw_parts(ptr, len)) as i32
}

#[no_mangle]
pub extern "C" fn out_ptr() -> *const u8 {
    OUT.with(|o| o.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn out_len() -> usize {
    OUT.with(|o| o.borrow().len())
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_non_ahx() {
        assert!(!super::decode_to_out(&[0u8; 16]));
        assert_eq!(super::out_len(), 0);
    }
}
