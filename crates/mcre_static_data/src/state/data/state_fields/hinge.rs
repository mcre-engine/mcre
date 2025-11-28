static VALUES: [u8; 3709] = *include_bytes!("./hinge.bin");

pub(crate) fn get(idx: u16) -> u8 {
    let byte = idx / 8;
    let bit = idx % 8;

    let byte = VALUES[byte as usize];

    (byte >> bit) & 1
}
