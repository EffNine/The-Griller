pub mod fact {
    pub fn record(claim: &str) -> bool {
        claim.contains("evidence")
    }
}

pub fn verify_claim(claim: &str) -> bool {
    fact::record(claim)
}
