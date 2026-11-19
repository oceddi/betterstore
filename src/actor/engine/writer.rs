use log::info;
use super::index::{Index, IndexElement};
use super::chunk::LogChunk;
use super::event::Event;


pub struct Writer {
  dir     : String,
  wchunk  : LogChunk,
  next_id : u64
}

impl Writer {
  pub fn new(dir: &str, mut wchunk_option : Option<LogChunk>, next_id: u64) -> Self {

    // Empty State?
    if wchunk_option.is_none() {
      info!("No chunks found in {}, starting an empty store", dir);
      wchunk_option = Some(LogChunk::new(1, LogChunk::path_for(dir, 1).as_str()));
    }

    Self {
      dir    : dir.to_string(),
      wchunk : wchunk_option.unwrap(),
      next_id
    }
  }

  pub fn append_events(&mut self, index: &mut Index, stream_name: String, events: Vec<String> ) -> Result<u64, String> {

    let events : Vec<Event> = events.iter().enumerate()
      .map(|(i, event_string)| Event::new(self.next_id + i as u64, &stream_name, event_string))
      .collect();

    // Reject the whole batch up front if any event could never fit in a chunk, so nothing
    // is partially written.
    if let Some(event) = events.iter().find(|event| !LogChunk::fits_in_empty_chunk(event)) {
      return Err(format!(
        "Event {} of stream {} exceeds the maximum event size of {} bytes.",
        event.id - self.next_id, stream_name, LogChunk::max_event_size()
      ));
    }

    for (i, event) in events.iter().enumerate() {
      // Attempt to commit event to chunk only failing if chunk is full.
      let mut write_result = self.wchunk.attempt_to_write_event(event, i == events.len()-1);
      if write_result.is_err() {
        info!("Chunk {} full, rolling over to chunk {}", self.wchunk.id, self.wchunk.id + 1);
        // Allocate another chunk file.
        let next_chunk_id = self.wchunk.id + 1;
        self.wchunk = LogChunk::new(next_chunk_id, LogChunk::path_for(&self.dir, next_chunk_id).as_str());

        write_result = self.wchunk.attempt_to_write_event(event, i == events.len()-1);
        if write_result.is_err() {
          return Err(format!("Failed to write event to new chunk file!"));
        }
      }
      // Add to index
      let index_element = IndexElement{
        chunk_number : self.wchunk.id,
        offset : write_result.unwrap()
      };

      index.add(&stream_name, index_element);
      self.next_id +=1;
    }

    Ok(self.next_id)
  }
}
