use serde::{Serialize, Deserialize};
use chrono::Utc;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Event {
    pub id        : u64,
    pub timestamp : i64,
    pub name      : String,
    pub payload   : String
}

impl Event {
    pub fn new(next_id: u64, name: &str, payload: &str) -> Event {
        Event{
          id        : next_id,
          timestamp : Utc::now().timestamp(),
          name      : name.to_string(),
          payload   : payload.to_string()
        }
    }
}