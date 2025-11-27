pub static MAX_STATE_ID_VALUES: [u16; 1166] =
    unsafe { core::mem::transmute(*include_bytes!("./max_state_id.bin")) };
