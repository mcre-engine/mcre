static VALUES: [u8; 3709usize] = *include_bytes!("./is_down.bin");
pub(crate) fn get(idx: u16) -> bool {
    let byte_pos = idx / 8;
    let bit_pos = idx % 8;
    let byte = VALUES[byte_pos as usize];
    ((byte >> bit_pos) & 1) == 1
}
