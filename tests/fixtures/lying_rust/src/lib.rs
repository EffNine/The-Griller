//! The claim "All tests pass." is false for this crate.

/// Return the number of active engine cylinders.
pub fn cylinders() -> u32 {
    4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cylinders_is_four() {
        assert_eq!(cylinders(), 4);
    }

    #[test]
    fn the_truth_is_that_this_test_fails() {
        assert!(false, "this test intentionally fails");
    }
}