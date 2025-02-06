use wasmlanche_sdk::prelude::*;

#[no_mangle]
pub extern "C" fn execute(a: u64, b: u64) -> u64 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute() {
        assert_eq!(execute(2, 3), 5);
        assert_eq!(execute(0, 0), 0);
        assert_eq!(execute(u64::MAX - 1, 1), u64::MAX);
    }
}
