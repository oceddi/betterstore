use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use log::{debug, info};
use regex::Regex;
use super::chunk::LogChunk;

#[derive(Debug, Clone, Copy)]
pub struct IndexElement {
  pub chunk_number : u32,
  pub offset : u32
}

pub struct Index {
  map : HashMap<String, Vec<IndexElement>>
}

impl Index {
  pub fn new() -> Self {
    Self{
      map : HashMap::new()
    }
  }

  pub fn initialize(&mut self, dir_name: &str) -> (Option<LogChunk>, u64) {
    let mut last_chunk : Option<LogChunk> = None;
    let mut next_id    : u64              = 0;
    let re                          = Regex::new(r"^(\d+)\.chk$").unwrap();

    // Create chunks folder if not already there...
    fs::create_dir_all(dir_name).ok();

    // Collect every chunk file with its id, skipping anything else in the directory
    // (e.g. .DS_Store), and sort by chunk id.
    let mut all_chunks : Vec<(u32, PathBuf)> = fs::read_dir(dir_name).unwrap()
      .filter_map(|entry| {
        let path     = entry.ok()?.path();
        let captures = re.captures(path.file_name()?.to_str()?)?;
        let chunk_id = captures.get(1)?.as_str().parse::<u32>().ok()?;
        Some((chunk_id, path))
      })
      .collect();
    all_chunks.sort_by_key(|(chunk_id, _)| *chunk_id);

    // Enumerate sorted log chunk files and load into index
    let chunk_count = all_chunks.len();
    for (i, (chunk_id, chunk_path)) in all_chunks.into_iter().enumerate() {
      let chunk_path_str = chunk_path.to_str().unwrap();

      debug!("Indexing chunk {} at {:?}", chunk_id, chunk_path);

      // Only the write chunk (the last one) can be left mid-write by a crash; earlier chunks
      // were fully committed when they rolled over.
      let is_write_chunk = i == chunk_count - 1;
      let (log_chunk, event_info) = LogChunk::index(chunk_id, chunk_path_str, is_write_chunk);

      for (i, (stream_name, _)) in event_info.iter().enumerate() {
        let index_element = IndexElement{
          chunk_number: log_chunk.id,
          offset : *log_chunk.offsets.get(i).unwrap()
        };

        self.add(stream_name, index_element);
      }

      // Track next_id across all chunks, not just the last, since the last chunk can be
      // empty (e.g. crash right after rollover).
      if let Some((_, last_id)) = event_info.last() {
        next_id = next_id.max(last_id + 1);
      }

      // Chunks are sorted, so the final one is the write chunk.
      last_chunk = Some(log_chunk);
    }
    info!("Indexed {} chunks, {} streams, next event id {}", chunk_count, self.map.len().saturating_sub(1), next_id);
    (last_chunk, next_id)
  }

  pub fn add(&mut self, stream_name: &str, value: IndexElement) {
    // Target specified stream first, then everything also always gets put in the $all stream.
    // Look up before inserting so the common case doesn't allocate a key per event.
    for name in [stream_name, "$all"] {
      match self.map.get_mut(name) {
        Some(entries) => entries.push(value),
        None => {
          debug!("Created {} stream", name);
          self.map.insert(name.to_string(), vec![value]);
        }
      }
    }
  }

  pub fn fetch_one(&self, stream_name: &str) -> &[IndexElement] {
    self.map.get(stream_name).map_or(&[], |entries| entries.as_slice())
  }
}
