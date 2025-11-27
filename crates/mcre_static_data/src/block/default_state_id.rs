pub static DEFAULT_STATE_ID_VALUES: [u16; 1166] =
    unsafe { core::mem::transmute(*include_bytes!("./default_state_id.bin")) };
