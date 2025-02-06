use wasmlanche::{Context, public};

#[public]
pub fn add(_context: &mut Context, a: i32, b: i32) -> i32 {
    a + b
}

#[public]
pub fn execute(_context: &mut Context) -> i32 {
    42
}
