use serde::{Serialize, Deserialize};
use chrono::Utc;

#[derive(Serialize, Deserialize, Debug)]
pub struct Event {
    pub id        : u64,
    pub timestamp : i64,
    pub name      : String,
    pub payload   : String
}

impl Event {
    pub fn new(next_id: u64, name: &str, payload: &str) -> Result<Event, &'static str> {
        Ok(Event{
          id        : next_id,
          timestamp : Utc::now().timestamp(),
          name      : name.to_string(),
          payload   : payload.to_string()
        })
    }
}

impl Clone for Event {
    fn clone(&self) -> Self {
        Self {
            id : self.id,
            timestamp: self.timestamp,
            name : self.name.clone(),
            payload : self.payload.clone()
        }
    }
}