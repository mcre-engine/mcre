static VALUES: [u8; 29671usize] = *include_bytes!("./age.bin");
pub(crate) fn get(idx: u16) -> u8 {
    VALUES[idx as usize]
}
