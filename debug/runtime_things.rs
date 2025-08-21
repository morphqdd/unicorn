#[no_mangle]
pub extern "C" fn stdprint(val: i64) {
    println!("{val}")
}

#[no_mangle]
pub extern "C" fn add(a: i64, b: i64) -> i64 {
    a + b
}