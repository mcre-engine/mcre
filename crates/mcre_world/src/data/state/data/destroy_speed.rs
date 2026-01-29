static VALUES: [f32; 29671usize] =
    unsafe { core::mem::transmute(*include_bytes!("./destroy_speed.bin")) };
pub(crate) fn get(idx: u16) -> f32 {
    VALUES[idx as usize]
}
