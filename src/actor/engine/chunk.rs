use std::fs::{self as fs, OpenOptions, File};
use std::io::{self, Read};
use std::os::unix::prelude::FileExt;
use std::path::Path;
use chrono::Utc;
use ring::digest::{Context, SHA256, SHA256_OUTPUT_LEN};
use serde::{Serialize, Deserialize};
use std::mem;
use bincode;

use super::event::Event;

const MAX_CHUNK_SIZE: u32 = 1000000; // 1 MB
const HEADER_VERSION: u8 = 1;

// Chunk layout: header, then records of (u32 length, bincode Event).
//
// The header hash covers the header (with a zeroed hash field) plus every committed record.
// It is only rewritten after a batch's records are synced, so it acts as the commit marker:
// on startup, records past the last boundary matching the hash are an interrupted write.
#[derive(Serialize, Deserialize, Debug)]
pub struct ChunkHeader {
  hash      : [u8; SHA256_OUTPUT_LEN],
  version   : u8,
  timestamp : i64
}

impl ChunkHeader {
  fn size_of () -> u32 {
    let header = Self {
      hash : [0; SHA256_OUTPUT_LEN],
      version   : HEADER_VERSION,
      timestamp : Utc::now().timestamp()
    };
    bincode::serialize(&header).unwrap().len() as u32
  }
}

pub struct LogChunk {
  pub id      : u32,
  pub offsets : Vec<u32>,
  available   : u32,
  handle      : File,
  context     : Context
}

impl LogChunk {

  pub fn path_for(dir: &str, id: u32) -> String {
    format!("{}/{}.chk", dir, id)
  }

  // Largest serialized event that fits in a freshly created chunk.
  pub fn max_event_size() -> u32 {
    MAX_CHUNK_SIZE - ChunkHeader::size_of() - mem::size_of::<u32>() as u32
  }

  pub fn fits_in_empty_chunk(event: &Event) -> bool {
    bincode::serialized_size(event).unwrap() <= Self::max_event_size() as u64
  }

  pub fn new(id: u32, path: &str) -> Self {

    // Create file as log chunk.  Handle will close after going out of scope.
    // Not opened in append mode: all writes are positional (write_all_at), and on Linux
    // O_APPEND makes pwrite ignore the offset, which would append the header hash.
    let handle = OpenOptions::new()
      .read(true)
      .write(true)
      .create_new(true)
      .open(&path).expect("Failed to create LogChunk file!");

    // Sync the directory so the new file itself survives a crash, not just its contents.
    if let Some(dir) = Path::new(path).parent().filter(|dir| !dir.as_os_str().is_empty()) {
      File::open(dir).and_then(|dir| dir.sync_all()).expect("Failed to sync chunk directory.");
    }

    let context = Self::write_empty_header(&handle);

    Self {
      id,
      offsets   : Vec::new(),
      available : MAX_CHUNK_SIZE - ChunkHeader::size_of(),
      handle,
      context
    }
  }

  // Writes a header for an empty chunk and returns the running hash context.
  fn write_empty_header(handle: &File) -> Context {
    let mut context = Context::new(&SHA256);

    // Hash is computed over the header with its hash field zero filled.
    let mut header = ChunkHeader{
      hash      : [0; SHA256_OUTPUT_LEN],
      version   : HEADER_VERSION,
      timestamp : Utc::now().timestamp()
    };
    context.update(&bincode::serialize(&header).unwrap());

    // Clone context so we can finish() and get the hash out w/o closing running context.
    header.hash.copy_from_slice(context.clone().finish().as_ref());

    handle.write_all_at(&bincode::serialize(&header).unwrap(), 0).expect("Failed to write header to LogChunk.");
    handle.sync_data().expect("Failed to sync LogChunk.");

    context
  }

  // Decodes the record at position, returning the event and the position just past it.
  fn decode_at(data: &[u8], position: usize) -> Result<(Event, usize), &'static str> {
    let len_end   = position + mem::size_of::<u32>();
    let len_bytes = data.get(position .. len_end).ok_or("Truncated event length")?;
    let encoded_len : u32 = bincode::deserialize(len_bytes).map_err(|_| "Unreadable event length")?;

    let end     = len_end + encoded_len as usize;
    let encoded = data.get(len_end .. end).ok_or("Truncated event")?;
    let event : Event = bincode::deserialize(encoded).map_err(|_| "Unreadable event")?;

    Ok((event, end))
  }

  // Loads an existing chunk.  When recover is set (only for the write chunk, the only one a
  // crash can leave mid-write) an incomplete header or uncommitted tail is truncated away.
  // Anything else that doesn't match the header hash is corruption and panics.
  pub fn index(id: u32, path: &str, recover: bool) -> (Self, Vec<(String, u64)>) {
    let mut handle = OpenOptions::new()
      .read(true)
      .write(true)
      .open(&path).expect("Failed to open LogChunk file!");

    let mut entire_file = Vec::with_capacity(MAX_CHUNK_SIZE as usize);
    handle.read_to_end(&mut entire_file).expect("Failed to read LogChunk file!");

    let header_size = ChunkHeader::size_of() as usize;

    // Crash while creating the chunk, before its header was synced.
    if entire_file.len() < header_size {
      if !recover {
        panic!("Corrupt log chunk {}: shorter than its header", path);
      }
      println!("Recovering {}: incomplete header, resetting to an empty chunk", path);
      handle.set_len(0).expect("Failed to truncate LogChunk.");
      let context = Self::write_empty_header(&handle);

      return (
        Self{
          id,
          offsets   : Vec::new(),
          available : MAX_CHUNK_SIZE - header_size as u32,
          handle,
          context
        },
        Vec::new()
      );
    }

    let mut stored_hash = [0; SHA256_OUTPUT_LEN];
    stored_hash.copy_from_slice(&entire_file[ .. SHA256_OUTPUT_LEN]);
    entire_file[ .. SHA256_OUTPUT_LEN].fill(0);

    let mut context = Context::new(&SHA256);
    context.update(&entire_file[ .. header_size]);

    // Walk records, remembering the last boundary whose running hash matches the stored hash.
    let mut events    = Vec::new(); // (offset, stream name, event id)
    let mut committed = None;       // (length, context, event count)
    let mut position  = header_size;
    loop {
      if context.clone().finish().as_ref() == stored_hash {
        committed = Some((position, context.clone(), events.len()));
      }

      match Self::decode_at(&entire_file, position) {
        Ok((event, next)) => {
          context.update(&entire_file[position .. next]);
          events.push((position as u32, event.name, event.id));
          position = next;
        }
        Err(_) => break
      }
    }

    let (committed_len, context, committed_events) = match committed {
      Some(committed) => committed,
      None => panic!("Corrupt log chunk {}: contents don't match header hash", path)
    };

    if committed_len < entire_file.len() {
      if !recover {
        panic!("Corrupt log chunk {}: {} uncommitted bytes in a sealed chunk", path, entire_file.len() - committed_len);
      }
      println!(
        "Recovering {}: discarding {} bytes ({} events) from an interrupted write",
        path, entire_file.len() - committed_len, events.len() - committed_events
      );
      handle.set_len(committed_len as u64).expect("Failed to truncate LogChunk.");
      handle.sync_data().expect("Failed to sync LogChunk.");
    }
    events.truncate(committed_events);

    let offsets    = events.iter().map(|(offset, _, _)| *offset).collect();
    let event_info = events.into_iter().map(|(_, name, id)| (name, id)).collect();

    (
      Self{
        id,
        offsets,
        available : MAX_CHUNK_SIZE - committed_len as u32,
        handle,
        context
      },
      event_info
    )
  }

  fn flush_chunk(&mut self) {
    // Records must be on disk before the hash that commits them, or a crash could leave a
    // hash covering bytes that never made it.  File::flush() is a no-op, hence sync_data.
    self.handle.sync_data().expect("Failed to sync LogChunk.");

    let new_hash = self.context.clone().finish();
    self.handle.write_all_at(new_hash.as_ref(), 0).expect("Failed to write hash to LogChunk.");
    self.handle.sync_data().expect("Failed to sync LogChunk.");
  }

  pub fn attempt_to_write_event(&mut self, event: &Event, flush: bool) -> Result<u32, &'static str> {

    // Serialize the event to bytes
    let mut encoded = bincode::serialize(event).unwrap();
    let encoded_len = encoded.len() as u32;

    // Can it fit?
    if encoded_len + mem::size_of::<u32>() as u32 > self.available {
      // flush writes to current chunk just in case before returning.
      self.flush_chunk();
      return Err("No space left in this chunk.");
    }

    let offset = MAX_CHUNK_SIZE - self.available;
    self.offsets.push(offset);

    // Length followed by payload in bincode format.
    let mut payload: Vec<u8> = Vec::with_capacity((encoded_len + mem::size_of::<u32>() as u32) as usize);
    payload.append(bincode::serialize(&encoded_len).as_mut().unwrap());
    payload.append(encoded.as_mut());

    self.handle.write_all_at(&payload, offset as u64).expect("Failed to write event to LogChunk.");
    self.context.update(&payload);
    if flush {
      self.flush_chunk();
    }
    self.available -= encoded_len + mem::size_of::<u32>() as u32;

    Ok(offset)
  }

  pub fn stream_events_out(offset: u32, path: &str) -> Result<Vec<Event>, io::Error> {
    let entire_file = fs::read(path)?;

    let mut position = offset as usize;
    let mut events   = Vec::new();

    while position < entire_file.len() {
      let (event, next) = Self::decode_at(&entire_file, position).map_err(|what| io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{} in {} at position {}", what, path, position)
      ))?;

      events.push(event);
      position = next;
    }

    Ok(events)
  }
}

#[cfg(test)]
mod tests {
  use std::fs::{self, OpenOptions};
  use std::io::Write;
  use super::*;
  use crate::test_util::temp_dir;

  fn chunk_path(name: &str) -> String {
    let dir = temp_dir(name);
    fs::create_dir_all(&dir).unwrap();
    LogChunk::path_for(&dir, 1)
  }

  fn write(chunk: &mut LogChunk, id: u64, flush: bool) {
    chunk.attempt_to_write_event(&Event::new(id, "s", &format!("payload {}", id)), flush).unwrap();
  }

  fn ids(event_info: &[(String, u64)]) -> Vec<u64> {
    event_info.iter().map(|(_, id)| *id).collect()
  }

  fn append_bytes(path: &str, bytes: &[u8]) {
    OpenOptions::new().append(true).open(path).unwrap().write_all(bytes).unwrap();
  }

  #[test]
  fn clean_chunk_reindexes_unchanged() {
    let path = chunk_path("chunk_clean");
    let mut chunk = LogChunk::new(1, &path);
    write(&mut chunk, 0, false);
    write(&mut chunk, 1, true);
    let len = fs::metadata(&path).unwrap().len();
    drop(chunk);

    let (chunk, event_info) = LogChunk::index(1, &path, true);
    assert_eq!(ids(&event_info), vec![0, 1]);
    assert_eq!(chunk.offsets.len(), 2);
    assert_eq!(fs::metadata(&path).unwrap().len(), len);
  }

  #[test]
  fn uncommitted_events_are_discarded() {
    let path = chunk_path("chunk_uncommitted");
    let mut chunk = LogChunk::new(1, &path);
    write(&mut chunk, 0, true);
    let committed_len = fs::metadata(&path).unwrap().len();

    // Crash mid-batch: records written, hash never updated.
    write(&mut chunk, 1, false);
    write(&mut chunk, 2, false);
    drop(chunk);

    let (mut chunk, event_info) = LogChunk::index(1, &path, true);
    assert_eq!(ids(&event_info), vec![0]);
    assert_eq!(fs::metadata(&path).unwrap().len(), committed_len);

    // Chunk is writable after recovery and reindexes cleanly.
    write(&mut chunk, 1, true);
    drop(chunk);
    let (_, event_info) = LogChunk::index(1, &path, false);
    assert_eq!(ids(&event_info), vec![0, 1]);
  }

  #[test]
  fn torn_record_is_discarded() {
    let path = chunk_path("chunk_torn");
    let mut chunk = LogChunk::new(1, &path);
    write(&mut chunk, 0, true);
    drop(chunk);

    append_bytes(&path, &[200, 0, 0, 0, 1, 2, 3]);

    let (_, event_info) = LogChunk::index(1, &path, true);
    assert_eq!(ids(&event_info), vec![0]);
  }

  #[test]
  fn incomplete_header_resets_to_empty_chunk() {
    let path = chunk_path("chunk_no_header");
    fs::write(&path, [0u8; 10]).unwrap();

    let (mut chunk, event_info) = LogChunk::index(1, &path, true);
    assert!(event_info.is_empty());
    write(&mut chunk, 0, true);
    drop(chunk);

    let (_, event_info) = LogChunk::index(1, &path, false);
    assert_eq!(ids(&event_info), vec![0]);
  }

  #[test]
  #[should_panic(expected = "uncommitted bytes in a sealed chunk")]
  fn sealed_chunk_with_uncommitted_bytes_panics() {
    let path = chunk_path("chunk_sealed");
    let mut chunk = LogChunk::new(1, &path);
    write(&mut chunk, 0, true);
    write(&mut chunk, 1, false);
    drop(chunk);

    LogChunk::index(1, &path, false);
  }

  #[test]
  #[should_panic(expected = "contents don't match header hash")]
  fn corrupted_committed_data_panics() {
    let path = chunk_path("chunk_corrupt");
    let mut chunk = LogChunk::new(1, &path);
    write(&mut chunk, 0, true);
    drop(chunk);

    // Flip the last byte of the committed record's payload.
    let mut bytes = fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    fs::write(&path, bytes).unwrap();

    LogChunk::index(1, &path, true);
  }
}
