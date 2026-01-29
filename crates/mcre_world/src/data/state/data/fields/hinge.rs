static VALUES: [u8; 3709usize] = *include_bytes!("./hinge.bin");
pub(crate) fn get(idx: u16) -> u8 {
    let byte_pos = idx / 8u16;
    let bit_pos = idx % 8u16;
    let byte = VALUES[byte_pos as usize];
    (byte >> bit_pos) & 1u8
}
