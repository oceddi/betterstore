use super::super::api::ReadStreamResponse;

use index::Index;
use writer::Writer;
use reader::ReaderStream;

pub mod event;
mod chunk;
mod index;
mod writer;
mod reader;

pub struct Engine {
  dir     : String,
  index   : Index,
  writer  : Writer
}

impl Engine {
  pub fn new(dir: &str) -> Self {

    let mut index                       = Index::new();
    let (wchunk, next_id) = index.initialize(dir);

    let writer = Writer::new(dir, wchunk, next_id);

    Self {
      dir : dir.to_string(),
      index,
      writer
    }
  }

  pub fn append_events(&mut self, stream_name: String, events: Vec<String>) -> Result<u64, String> {
    // You can't append events to certain "reserved" stream names.
    match stream_name.as_str() {
      "$all" => Err(format!("Illegal stream_name parameter.")),
      _ => {
        // Returns next_id
        self.writer.append_events(
          &mut self.index,
          stream_name,
          events
        )
      }
    }
  }

  pub fn read_stream(&self, stream_name: &str, stream_position: u64) -> Result<Vec<ReadStreamResponse>, std::io::Error> {
    let reader = ReaderStream::new(&self.dir, stream_name, &self.index);

    reader.read_stream(stream_position)
  }

}

#[cfg(test)]
mod tests {
  use std::fs::{self, OpenOptions};
  use std::io::Write;
  use super::*;
  use super::event::Event;
  use crate::test_util::temp_dir;

  fn payloads(prefix: &str, count: usize) -> Vec<String> {
    (0..count).map(|i| format!("{} {}", prefix, i)).collect()
  }

  fn read(engine: &Engine, stream_name: &str, start: u64) -> Vec<(String, u64)> {
    engine.read_stream(stream_name, start).unwrap()
      .into_iter()
      .map(|r| (r.event, r.stream_position))
      .collect()
  }

  #[test]
  fn events_and_ids_survive_restart() {
    let dir = temp_dir("restart");

    {
      let mut engine = Engine::new(&dir);
      assert_eq!(engine.append_events("a".to_string(), payloads("a", 3)), Ok(3));
      assert_eq!(engine.append_events("b".to_string(), payloads("b", 2)), Ok(5));
    }

    // Re-indexing verifies each chunk's hash, so this also checks the header hash was
    // written in place rather than appended.
    let mut engine = Engine::new(&dir);
    assert_eq!(engine.append_events("a".to_string(), payloads("a2", 1)), Ok(6));

    // Positions are per-stream, not global event ids.
    assert_eq!(read(&engine, "a", 0), vec![
      ("a 0".to_string(), 0), ("a 1".to_string(), 1), ("a 2".to_string(), 2), ("a2 0".to_string(), 3)
    ]);
    assert_eq!(read(&engine, "b", 1), vec![("b 1".to_string(), 1)]);
    assert_eq!(read(&engine, "$all", 4), vec![("b 1".to_string(), 4), ("a2 0".to_string(), 5)]);
  }

  #[test]
  fn restart_after_interrupted_write_recovers() {
    let dir = temp_dir("interrupted");

    {
      let mut engine = Engine::new(&dir);
      engine.append_events("a".to_string(), payloads("a", 2)).unwrap();
    }

    // Simulate a crash mid-batch: a full record and a torn one past the committed hash.
    let event   = Event::new(2, "a", "never committed");
    let encoded = bincode::serialize(&event).unwrap();
    let mut tail = bincode::serialize(&(encoded.len() as u32)).unwrap();
    tail.extend(encoded);
    tail.extend([50, 0, 0, 0, 1, 2]);
    OpenOptions::new().append(true).open(format!("{}/1.chk", dir)).unwrap().write_all(&tail).unwrap();

    let mut engine = Engine::new(&dir);
    assert_eq!(engine.append_events("a".to_string(), payloads("b", 1)), Ok(3));
    drop(engine);

    let engine = Engine::new(&dir);
    assert_eq!(read(&engine, "a", 0), vec![
      ("a 0".to_string(), 0), ("a 1".to_string(), 1), ("b 0".to_string(), 2)
    ]);
  }

  #[test]
  fn empty_store_restart_keeps_ids_starting_at_zero() {
    let dir = temp_dir("empty_restart");
    drop(Engine::new(&dir));

    let mut engine = Engine::new(&dir);
    engine.append_events("a".to_string(), payloads("a", 1)).unwrap();
    assert_eq!(read(&engine, "a", 0), vec![("a 0".to_string(), 0)]);
  }

  #[test]
  fn reads_span_chunk_rollover() {
    let dir     = temp_dir("rollover");
    let big     = "x".repeat(100_000);
    let mut engine = Engine::new(&dir);

    // ~1.2MB across two streams forces at least one new chunk.
    for i in 0..12 {
      let stream_name = if i % 2 == 0 { "even" } else { "odd" };
      engine.append_events(stream_name.to_string(), vec![format!("{} {}", i, big)]).unwrap();
    }
    assert!(fs::metadata(format!("{}/2.chk", dir)).is_ok());

    let positions = |stream_name: &str, start: u64| -> Vec<u64> {
      read(&engine, stream_name, start).into_iter().map(|(_, position)| position).collect()
    };
    assert_eq!(positions("even", 0), vec![0, 1, 2, 3, 4, 5]);
    assert_eq!(positions("odd", 4), vec![4, 5]);
    assert_eq!(positions("$all", 5), (5..12).collect::<Vec<u64>>());

    drop(engine);
    let engine = Engine::new(&dir);
    assert_eq!(read(&engine, "$all", 0).len(), 12);
  }

  #[test]
  fn oversized_event_is_rejected_without_writing() {
    let dir = temp_dir("oversized");
    let mut engine = Engine::new(&dir);

    let batch = vec!["small".to_string(), "x".repeat(2_000_000)];
    assert!(engine.append_events("a".to_string(), batch).is_err());
    assert!(read(&engine, "a", 0).is_empty());
    assert!(fs::metadata(format!("{}/2.chk", dir)).is_err());

    // Store is still usable and ids weren't consumed.
    assert_eq!(engine.append_events("a".to_string(), payloads("a", 1)), Ok(1));
  }

  #[test]
  fn appending_to_all_is_rejected() {
    let dir = temp_dir("append_all");
    let mut engine = Engine::new(&dir);
    assert!(engine.append_events("$all".to_string(), payloads("a", 1)).is_err());
  }

  #[test]
  fn unknown_stream_reads_empty() {
    let dir = temp_dir("unknown");
    let engine = Engine::new(&dir);
    assert!(read(&engine, "nope", 0).is_empty());
  }

  #[test]
  fn stray_files_in_chunk_dir_are_ignored() {
    let dir = temp_dir("stray");
    fs::create_dir_all(&dir).unwrap();
    fs::write(format!("{}/.DS_Store", dir), b"junk").unwrap();

    let mut engine = Engine::new(&dir);
    engine.append_events("a".to_string(), payloads("a", 1)).unwrap();
    drop(engine);

    let engine = Engine::new(&dir);
    assert_eq!(read(&engine, "a", 0).len(), 1);
  }
}
