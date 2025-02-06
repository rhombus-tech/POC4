use wasmlanche::{public, Context};

#[public]
pub fn add(context: &mut Context, a: i32, b: i32) -> i32 {
    a + b
}

#[public]
pub fn execute(context: &mut Context) -> i32 {
    add(context, 1, 2)
}
