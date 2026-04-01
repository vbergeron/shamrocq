pub fn as_words_mut(bytes: &mut [u8]) -> &mut [u32] {
    let ptr = bytes.as_mut_ptr();
    assert!(
        ptr as usize % 4 == 0,
        "buffer must be 4-byte aligned"
    );
    let word_len = bytes.len() / 4;
    unsafe { core::slice::from_raw_parts_mut(ptr as *mut u32, word_len) }
}

pub fn words_as_bytes(words: &[u32]) -> &[u8] {
    let ptr = words.as_ptr() as *const u8;
    let byte_len = words.len() * 4;
    unsafe { core::slice::from_raw_parts(ptr, byte_len) }
}

pub fn words_as_bytes_mut(words: &mut [u32]) -> &mut [u8] {
    let ptr = words.as_mut_ptr() as *mut u8;
    let byte_len = words.len() * 4;
    unsafe { core::slice::from_raw_parts_mut(ptr, byte_len) }
}
