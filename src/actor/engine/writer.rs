use super::index::{Index, IndexElement};
use super::chunk::LogChunk;
use super::event::Event;


pub struct Writer {
  wchunk  : LogChunk,
  next_id : u64
}

impl Writer {
  pub fn new(mut wchunk_option : Option<LogChunk>, next_id: u64) -> Self {

    // Empty State?
    if wchunk_option.is_none() {
      println!("Starting from Empty!");
      wchunk_option = Some(LogChunk::new(1, "chunks/1.chk"));
    }

    Self {
      wchunk : wchunk_option.unwrap(),
      next_id
    }
  }

  pub fn append_events(&mut self, index: &mut Index, stream_name: String, events: Vec<String> ) -> u64 {

    for (i, event_string) in events.iter().enumerate() {
      let event = &Event::new(
        self.next_id,
        &stream_name,
        event_string
      ).unwrap();

      // Attempt to commit event to chunk only failing if chunk is full.
      let mut write_result = self.wchunk.attempt_to_write_event(event, i == events.len()-1);
      if write_result.is_err() {
        println!("Chunk {} full!", self.wchunk.id);
        // Allocate another chunk file.
        let next_chunk_id = self.wchunk.id + 1;
        self.wchunk = LogChunk::new(next_chunk_id, format!("chunks/{}.chk", next_chunk_id).as_str());

        write_result = self.wchunk.attempt_to_write_event(event, i == events.len()-1);
        if write_result.is_err() {
          panic!("Failed to write event to new chunk file!");
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

    self.next_id
  }
}
