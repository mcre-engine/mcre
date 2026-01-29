static VALUES: [u8; 29671usize] = *include_bytes!("./note.bin");
pub(crate) fn get(idx: u16) -> u8 {
    VALUES[idx as usize]
}
