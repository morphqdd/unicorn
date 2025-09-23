use std::time::Instant;

#[no_mangle]
pub extern "C" fn stdprint(val: i64) {
    println!("{val}")
}

#[no_mangle]
pub extern "C" fn add(a: i64, b: i64) -> i64 {
    a + b
}

#[no_mangle]
pub unsafe extern "C" fn now() -> *const Instant {
    let start = Box::new(Instant::now());
    Box::into_raw(start) as *const _
}

#[no_mangle]
pub unsafe extern "C" fn elapsed(instant: *const Instant) -> i64 {
    let instant: Box<Instant> = unsafe { Box::from_raw(instant as *mut _) };
    instant.elapsed().as_micros() as i64
}
