static VALUES: [u16; 1166usize] = unsafe {
    core::mem::transmute(*include_bytes!("./min_state_id.bin"))
};
pub(crate) fn get(idx: u16) -> u16 {
    VALUES[idx as usize]
}
