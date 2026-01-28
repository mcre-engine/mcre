static VALUES: [u16; 29673usize] = unsafe { core::mem::transmute(*include_bytes!("./block.bin")) };
pub(crate) fn get(idx: u16) -> u16 {
    VALUES[idx as usize]
}
