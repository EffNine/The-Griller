//! greet_rust library root.

pub mod greet;

/// Greet the world.
pub fn hello() -> &'static str {
    "hello"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_works() {
        assert_eq!(hello(), "hello");
    }
}