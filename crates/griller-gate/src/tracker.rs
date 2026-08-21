//! Loop prevention: at most one gate invocation per completion event.
//!
//! A completion event is identified by the host's own boundary marker — in
//! the OpenCode adapter that is the assistant message ID of the turn that
//! ended. The tracker records the marker of the last event already presented
//! to the gate; presenting the same marker again is a no-op. When the agent
//! later produces a *new* completion attempt (remediation), it carries a new
//! marker and is verified again — re-attempts are supported, infinite loops
//! are not.
//!
//! The gate itself never emits a completion event, so the tracker has no
//! path that re-triggers the gate from the gate's own output.

use std::collections::HashMap;

/// Per-session tracking of which completion event was already gated.
#[derive(Debug, Default, Clone)]
pub struct GateTracker {
    /// session identity -> last completion marker already presented to the gate.
    verified: HashMap<String, String>,
}

impl GateTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// True when this completion event has not been gated yet.
    ///
    /// A marker of `None` (host without a boundary marker) is always treated
    /// as new; hosts without markers must implement their own deduplication.
    pub fn is_new(&self, session: &str, marker: Option<&str>) -> bool {
        match marker {
            Some(marker) => self
                .verified
                .get(session)
                .map(|m| m != marker)
                .unwrap_or(true),
            None => true,
        }
    }

    /// Record that a completion event was presented to the gate.
    pub fn mark_verified(&mut self, session: &str, marker: Option<&str>) {
        if let Some(marker) = marker {
            self.verified
                .insert(session.to_string(), marker.to_string());
        }
    }

    /// Number of tracked sessions (diagnostic).
    pub fn len(&self) -> usize {
        self.verified.len()
    }

    pub fn is_empty(&self) -> bool {
        self.verified.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_event_is_gated_exactly_once() {
        let mut tracker = GateTracker::new();
        let session = "s1";
        assert!(tracker.is_new(session, Some("m1")));
        tracker.mark_verified(session, Some("m1"));
        // The same completion boundary (same marker) must not run again.
        assert!(!tracker.is_new(session, Some("m1")));
        assert!(!tracker.is_new(session, Some("m1")));
    }

    #[test]
    fn new_attempt_after_block_is_verified_again() {
        let mut tracker = GateTracker::new();
        let session = "s1";
        tracker.mark_verified(session, Some("attempt-1"));
        assert!(!tracker.is_new(session, Some("attempt-1")));
        // The agent remediated and produced a new completion event.
        assert!(tracker.is_new(session, Some("attempt-2")));
        tracker.mark_verified(session, Some("attempt-2"));
        assert!(!tracker.is_new(session, Some("attempt-2")));
    }

    #[test]
    fn distinct_sessions_are_independent() {
        let mut tracker = GateTracker::new();
        tracker.mark_verified("s1", Some("m1"));
        assert!(tracker.is_new("s2", Some("m1")));
        assert_eq!(tracker.len(), 1);
    }

    #[test]
    fn host_without_marker_is_always_new() {
        let mut tracker = GateTracker::new();
        tracker.mark_verified("s1", None);
        assert!(tracker.is_new("s1", None));
    }
}
