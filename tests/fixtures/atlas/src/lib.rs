pub mod fact_store;

pub fn ingest() {
    let _ = fact_store::store_fact("hello");
}
