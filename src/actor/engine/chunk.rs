use std::fs::{self as fs, OpenOptions, File};
use std::io::{Read, Write};
use std::mem;
use bincode;

use super::event::Event;

const MAX_CHUNK_SIZE: u32 = 1000000; // 1 MB

#[derive(Clone)]
pub struct LogChunk {
  pub id      : u32,
  pub offsets : Vec<u32>,
  available   : u32,
  handle      : FileWrapper
}

pub struct FileWrapper {
  handle      : File
}

impl Clone for FileWrapper {
    fn clone(&self) -> Self {
        Self { handle: self.handle.try_clone().unwrap() }
    }

    fn clone_from(&mut self, source: &Self) {
        Self { handle: source.handle.try_clone().unwrap() };
    }
}

impl LogChunk {

  pub fn new(id: u32, path: &str) -> Self {

    // Create file as log chunk.  Handle will close after going out of scope.
    let handle = OpenOptions::new()
      .read(true)
      .append(true)
      .create_new(true)
      .open(&path).expect("Failed to create LogChunk file!");

    Self{
      id,
      offsets   : Vec::new(),
      available : MAX_CHUNK_SIZE,
      handle    : FileWrapper { handle }
    }
  }

  pub fn index(id: u32, path: &str) -> (Self, Vec<(String, u64)>) {

    let mut handle = OpenOptions::new()
      .read(true)
      .append(true)
      .open(&path).expect("Failed to open LogChunk file!");

    let mut entire_file      = Vec::with_capacity(MAX_CHUNK_SIZE as usize);
    let chunk_len = handle.read_to_end(entire_file.as_mut());

    let mut position  : u32 = 0;
    let mut remaining : u32 = chunk_len.unwrap() as u32;
    let available   = MAX_CHUNK_SIZE - remaining;

    let mut event_info = Vec::new();
    let mut offsets         = Vec::new();

    loop {
      offsets.push(position);

      let range_len     = std::ops::Range{ start: position as usize, end: (position + 4) as usize};
      let encoded_len : u32         = bincode::deserialize(entire_file.get(range_len).unwrap()).unwrap();
      position += mem::size_of::<u32>() as u32;

      let range_encoded = std::ops::Range{ start: position as usize, end: (position + encoded_len) as usize};
      let event : Event             = bincode::deserialize(entire_file.get(range_encoded).unwrap()).unwrap();

      event_info.push((event.name, event.id));

      position  += encoded_len;
      remaining -= encoded_len + mem::size_of::<u32>() as u32;

      if remaining == 0 {
        break;
      }
    }

    (
      Self{
        id,
        offsets,
        available,
        handle : FileWrapper { handle }
      },
      event_info
    )
  }

  pub fn attempt_to_write_event(&mut self, event: &Event, flush: bool) -> Result<u32, &'static str> {

    // Serialize the event to bytes
    let mut encoded = bincode::serialize(event).unwrap();
    let encoded_len = encoded.len() as u32;

    // Can it fit?
    if encoded_len + mem::size_of::<u32>() as u32 > self.available {
      // flush writes to current chunk just in case before returning.
      let mut file = &self.handle.handle;

      file.flush().expect("Failed to flush event to LogChunk.");

      return Err("No space left in this chunk.");
    }

    let offset = MAX_CHUNK_SIZE - self.available;
    self.offsets.push(offset);

    // Length followed by payload in bincode format.
    //println!("Writing {} bytes + 4 in chunk {}", encoded_len, self.id);
    let mut payload: Vec<u8> = Vec::with_capacity((encoded_len + mem::size_of::<u32>() as u32) as usize);
    payload.append(bincode::serialize(&encoded_len).as_mut().unwrap());
    payload.append(encoded.as_mut());
    let mut file = &self.handle.handle;

    file.write_all(&payload).expect("Failed to write event to LogChunk.");
    if flush {
      file.flush().expect("Failed to flush event to LogChunk.");
    }
    self.available -= encoded_len + mem::size_of::<u32>() as u32;

    Ok(offset)
  }

  pub fn stream_events_out(offset: u32, path: &str) -> Result<Vec<Event>, std::io::Error> {
    let entire_file = fs::read(path)?;
    let chunk_len     = entire_file.len();

    let mut position  : u32 = offset;
    let mut remaining : u32 = chunk_len as u32 - offset;

    let mut events = Vec::new();

    loop {
      let range_len = std::ops::Range{ start: position as usize, end: (position + 4) as usize};
      let encoded_len : u32     = bincode::deserialize(entire_file.get(range_len).unwrap()).unwrap();

      position += mem::size_of::<u32>() as u32;

      let range_encoded = std::ops::Range{ start: position as usize, end: (position + encoded_len) as usize};
      let event : Event             = bincode::deserialize(entire_file.get(range_encoded).unwrap()).unwrap();

      events.push(event);

      position  += encoded_len;
      remaining -= encoded_len + mem::size_of::<u32>() as u32;

      if remaining == 0 {
        break;
      }
    }

    Ok(events)
  }
}
