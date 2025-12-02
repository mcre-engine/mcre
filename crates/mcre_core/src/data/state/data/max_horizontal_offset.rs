static VALUES: [f32; 29671usize] = unsafe {
    core::mem::transmute(*include_bytes!("./max_horizontal_offset.bin"))
};
pub(crate) fn get(idx: u16) -> f32 {
    VALUES[idx as usize]
}
