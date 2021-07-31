use chrono::{DateTime, Utc};
use serde::Serialize;

/// in-memory reconciler state exposed on /state
#[derive(Clone, Serialize)]
pub struct State {
    #[serde(deserialize_with = "from_ts")]
    pub last_event: DateTime<Utc>,
}

impl State {
    pub(crate) fn new() -> Self {
        State {
            last_event: Utc::now(),
        }
    }
}
