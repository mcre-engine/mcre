static VALUES: [u8; 7418usize] = *include_bytes!("./flower_amount.bin");
pub(crate) fn get(idx: u16) -> u8 {
    let byte_pos = idx / 4u16;
    let bit_pos = (idx % 4u16) * 2u16;
    let byte = VALUES[byte_pos as usize];
    ((byte >> bit_pos) & 3u8) + 1u8
}
