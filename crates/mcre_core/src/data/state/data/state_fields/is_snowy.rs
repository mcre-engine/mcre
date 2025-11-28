static VALUES: [u8; 3709] = *include_bytes!("./is_snowy.bin");

pub(crate) fn get(idx: u16) -> bool {
    let byte = idx / 8;
    let bit = idx % 8;

    let byte = VALUES[byte as usize];

    ((byte >> bit) & 1) == 1
}
