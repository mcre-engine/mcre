static VALUES: [u128; 1166usize] =
    unsafe { core::mem::transmute(*include_bytes!("./fields_present.bin")) };
pub(crate) fn get(idx: u16) -> u128 {
    VALUES[idx as usize]
}
