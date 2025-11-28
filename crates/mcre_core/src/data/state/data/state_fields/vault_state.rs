static VALUES: [u8; 7418] = *include_bytes!("./vault_state.bin");

pub(crate) fn get(idx: u16) -> u8 {
    let byte = idx / 4;
    let bit = idx % 4;

    let byte = VALUES[byte as usize];

    (byte >> bit) & 3
}
