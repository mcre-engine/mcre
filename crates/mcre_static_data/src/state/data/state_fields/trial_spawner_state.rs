static VALUES: [u8; 14836] = *include_bytes!("./trial_spawner_state.bin");

pub(crate) fn get(idx: u16) -> u8 {
    let byte = idx / 2;
    let bit = idx % 2;

    let byte = VALUES[byte as usize];

    (byte >> bit) & 15
}
