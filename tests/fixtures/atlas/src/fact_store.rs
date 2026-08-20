pub struct FactStore;

pub fn store_fact(fact: &str) -> bool {
    fact.len() >= 3
}

pub fn query_fact(key: &str) -> Option<String> {
    if key.is_empty() {
        None
    } else {
        Some(format!("fact:{key}"))
    }
}
