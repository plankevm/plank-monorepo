pub const fn const_num_to_str(buf: &mut [u8; 20], mut x: u64) -> &str {
    // Safety: 10^20 > 2^64
    unsafe {
        buf[19] = b'0';

        let mut i = 20;
        while x > 0 {
            i -= 1;
            buf[i] = (x % 10) as u8 + b'0';
            x = x / 10;
        }

        let ptr: *mut u8 = buf.as_mut_ptr();
        let bytes = core::slice::from_raw_parts_mut(ptr.add(i), 20 - i);
        str::from_utf8_unchecked(bytes)
    }
}

pub const fn const_assert_eq(x: usize, y: usize) {
    if x == y {
        return;
    }
    let mut xbuf = [0u8; 20];
    let xs = const_num_to_str(&mut xbuf, x as u64);
    panic!("{}", xs);
}
