use std::fs::{self as fs, OpenOptions, File};
use std::io::{Read, Write};
use std::os::unix::prelude::FileExt;
use chrono::Utc;
use ring::digest::{Context, SHA256, SHA256_OUTPUT_LEN};
use serde::{Serialize, Deserialize};
use std::mem;
use bincode;

use super::event::Event;

const MAX_CHUNK_SIZE: u32 = 1000000; // 1 MB
const HEADER_VERSION: u8 = 1;

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

  pub fn new(id: u32, path: &str) -> Self {

    // Create file as log chunk.  Handle will close after going out of scope.
    let mut handle = OpenOptions::new()
      .read(true)
      .append(true)
      .create_new(true)
      .open(&path).expect("Failed to create LogChunk file!");

    // Setup hashing, hash starts out zero filled
    let hash = [0; SHA256_OUTPUT_LEN];
    let mut context = Context::new(&SHA256);

    let mut header = ChunkHeader{
      hash,
      version   : HEADER_VERSION,
      timestamp : Utc::now().timestamp()
    };

    let mut serialized_header = bincode::serialize(&header).unwrap();
    context.update(&serialized_header);

    // Clone context so we can finish() and get the hash out w/o closing running context.
    let context_clone = context.clone();

    // Start empty chunk with valid header
    header.hash.copy_from_slice(context_clone.finish().as_ref());
    
    serialized_header = bincode::serialize(&header).unwrap();

    handle.write_all(&serialized_header).expect("Failed to write header to LogChunk.");
    handle.flush().expect("Failed to flush event to LogChunk.");

    Self {
      id,
      offsets   : Vec::new(),
      available : MAX_CHUNK_SIZE-serialized_header.len() as u32,
      handle,
      context
    }
  }

  fn check_hash(context : &mut Context, file_data : &mut Vec<u8>) -> bool {
    let mut file_hash : [u8; SHA256_OUTPUT_LEN] = [0; SHA256_OUTPUT_LEN];
    let empty_hash : [u8; SHA256_OUTPUT_LEN]    = [0; SHA256_OUTPUT_LEN];
    
    file_hash.copy_from_slice(&file_data[ .. SHA256_OUTPUT_LEN]);

    file_data.splice( .. SHA256_OUTPUT_LEN, empty_hash);

    context.update(file_data);

    let context_clone = context.clone();

    let calc_hash = context_clone.finish();


    for (i, val) in file_hash.iter().enumerate() {
      if *val != calc_hash.as_ref()[i] {
        println!("Found hash : {:02x?}", file_hash);
        println!("Calc hash  : {:02x?}", calc_hash.as_ref());

        return false;
      }
    }

    return true;
  }

  pub fn index(id: u32, path: &str) -> (Self, Vec<(String, u64)>) {
    let mut context = Context::new(&SHA256);

    let mut handle = OpenOptions::new()
      .read(true)
      .append(true)
      .open(&path).expect("Failed to open LogChunk file!");

    let mut entire_file      = Vec::with_capacity(MAX_CHUNK_SIZE as usize);
    let chunk_len = handle.read_to_end(entire_file.as_mut());

    if !LogChunk::check_hash(
      &mut context, 
      &mut entire_file
    ) {
      panic!("Corrupt log chunk {}", path);
    }

    let header_size = ChunkHeader::size_of();
    let mut position  : u32 = header_size;
    let mut remaining : u32 = chunk_len.unwrap() as u32 - header_size;
    let available   = MAX_CHUNK_SIZE - remaining - header_size;

    let mut event_info = Vec::new();
    let mut offsets         = Vec::new();

    if remaining == 0 {
      println!("Empty chunk!");
      return (
        Self{
          id,
          offsets,
          available,
          handle,
          context
        },
        event_info
      );
    }


    loop {
      offsets.push(position);

      // Read each event by first reading its length then the event itself from the chunk
      let range_len     = std::ops::Range{ start: position as usize, end: (position + 4) as usize};
      let encoded_len : u32         = bincode::deserialize(entire_file.get(range_len).unwrap()).unwrap();
      position += mem::size_of::<u32>() as u32;

      let range_encoded = std::ops::Range{ start: position as usize, end: (position + encoded_len) as usize};
      let event_data_result : Option<&[u8]>= entire_file.get(range_encoded);

      // Error handling
      match event_data_result {
        Some(slice) => {
          let event_result : Result<Event, _> = bincode::deserialize(slice);
          match event_result {
            Ok(event) => {
              event_info.push((event.name, event.id));
            }
            Err(e) => {
              panic!("Failed to deserialize the event data: {}", e);
            }
          }
        }
        None => {
          panic!("Failed to find event data in range specified {} {}", position, encoded_len);
        }
      }

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
        handle,
        context
      },
      event_info
    )
  }

  fn flush_chunk(&mut self) {
    let context_clone = self.context.clone();
    let new_hash = context_clone.finish();
    let _ = self.handle.write_at(new_hash.as_ref(), 0).unwrap();

    self.handle.flush().expect("Failed to flush event to LogChunk.");
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
    //println!("Writing {} bytes + 4 in chunk {}", encoded_len, self.id);
    let mut payload: Vec<u8> = Vec::with_capacity((encoded_len + mem::size_of::<u32>() as u32) as usize);
    payload.append(bincode::serialize(&encoded_len).as_mut().unwrap());
    payload.append(encoded.as_mut());

    self.handle.write_all(&payload).expect("Failed to write event to LogChunk.");
    self.context.update(&payload);
    if flush {
      self.flush_chunk();
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
