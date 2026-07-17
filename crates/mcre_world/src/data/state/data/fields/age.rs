static VALUES: [u8; 34347usize] = *include_bytes!("./age.bin");
pub(crate) fn get(idx: u16) -> u8 {
    VALUES[idx as usize]
}
