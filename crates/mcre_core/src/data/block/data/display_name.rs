const COUNT: usize = 1168usize;
static DATA: &[u8] = include_bytes!("./display_name.bin");
pub(crate) fn get(idx: u16) -> &'static str {
    let idx = idx as usize;
    let get_offset = |i: usize| -> u32 {
        let start = i * core::mem::size_of::<u32>();
        let end = start + core::mem::size_of::<u32>();
        let bytes: [u8; core::mem::size_of::<u32>()] = DATA[start..end].try_into().unwrap();
        u32::from_ne_bytes(bytes)
    };
    let start = get_offset(idx) as usize;
    let end = get_offset(idx + 1) as usize;
    let strings_data_offset = (COUNT + 1) * core::mem::size_of::<u32>();
    let strings_data = &DATA[strings_data_offset..];
    let slice = &strings_data[start..end];
    unsafe { core::str::from_utf8_unchecked(slice) }
}
