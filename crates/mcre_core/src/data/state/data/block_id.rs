static VALUES: [u16; 29671usize] =
    unsafe { core::mem::transmute(*include_bytes!("./block_id.bin")) };
pub(crate) fn get(idx: u16) -> u16 {
    VALUES[idx as usize]
}
