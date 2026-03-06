// @liyi:related no-overflow
fn add(a: i32, b: i32) -> i32 {
    a.checked_add(b).expect("overflow")
}
