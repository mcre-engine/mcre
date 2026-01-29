static VALUES: [u8; 14836usize] = *include_bytes!("./trial_spawner_state.bin");
pub(crate) fn get(idx: u16) -> u8 {
    let byte_pos = idx / 2u16;
    let bit_pos = (idx % 2u16) * 4u16;
    let byte = VALUES[byte_pos as usize];
    (byte >> bit_pos) & 15u8
}
