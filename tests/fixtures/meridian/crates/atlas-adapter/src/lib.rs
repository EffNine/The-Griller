pub fn read_fact(key: &str) -> Option<String> {
    if key.is_empty() {
        None
    } else {
        Some(format!("atlas-fact:{key}"))
    }
}
