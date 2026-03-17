pub const fn const_num_to_str(buf: &mut [u8; 20], mut x: u64) -> &str {
    // Safety: 10^20 > 2^64
    unsafe {
        let mut i = 19;
        buf[i] = b'0';

        while x > 0 {
            buf[i] = (x % 10) as u8 + b'0';
            x /= 10;
            i -= 1;
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
