
use std::io;
use super::index::{Index, IndexElement};
use super::super::super::api::ReadStreamResponse;

use super::chunk::LogChunk;


pub struct ReaderStream<'stream> {
  dir           : &'stream str,
  stream_name   : &'stream str,
  index_entries : &'stream [IndexElement]
}

impl <'stream> ReaderStream<'stream> {
  pub fn new(dir: &'stream str, stream_name: &'stream str, index: &'stream Index) -> Self {
    let index_entries = index.fetch_one(stream_name);

    Self {
      dir,
      stream_name,
      index_entries
    }
  }

  pub fn read_stream(&self, start : u64) -> Result<Vec<ReadStreamResponse>, io::Error> {
    let mut responses = Vec::new();

    // Each chunk read returns every event from the indexed offset to the end of that
    // chunk, so one read covers all of this stream's index entries in the chunk.
    let mut current = start as usize;
    while current < self.index_entries.len() {
      let index_element = &self.index_entries[current];
      let chunk_path_str = LogChunk::path_for(self.dir, index_element.chunk_number);

      let chunk_events = LogChunk::stream_events_out(index_element.offset, chunk_path_str.as_str())?;

      let before = current;
      for event in chunk_events.iter().filter(|evn| self.stream_name == "$all" || evn.name == self.stream_name) {
        // Position within the requested stream, so a reader can resume from position + 1.
        responses.push(ReadStreamResponse {
          event : event.payload.clone(),
          stream_position : current as u64
        });
        current += 1;
      }

      // The indexed offset should always point at an event in this stream.
      if current == before {
        return Err(io::Error::new(
          io::ErrorKind::InvalidData,
          format!("Index entry for {} points at no matching event in {}", self.stream_name, chunk_path_str)
        ));
      }
    }

    Ok(responses)
  }
}
