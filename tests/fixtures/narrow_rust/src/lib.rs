//! Unit tests in this crate pass; the integration test does not.

/// Compute the square of a number.
pub fn square(x: i64) -> i64 {
    x * x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_works() {
        assert_eq!(square(3), 9);
        assert_eq!(square(-2), 4);
    }
}