pub static MIN_STATE_ID_VALUES: [u16; 1166] =
    unsafe { core::mem::transmute(*include_bytes!("./min_state_id.bin")) };
