//! Greeting module.

/// Greet someone by name.
pub fn greet(name: &str) -> String {
    format!("hello, {name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greets_by_name() {
        assert_eq!(greet("world"), "hello, world");
    }
}