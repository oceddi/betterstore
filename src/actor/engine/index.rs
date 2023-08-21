use std::collections::HashMap;
use std::fs;
use regex::Regex;
use super::chunk::LogChunk;

#[derive(Debug, Clone)]
pub struct IndexElement {
  pub chunk_number : u32,
  pub offset : u32
}

#[derive(Clone)]
pub struct Index {
  map : HashMap<String, Vec<IndexElement>>
}

impl Index {
  pub fn new() -> Self {
    println!("Creating new index/hashmap");
    Self{
      map : HashMap::new()
    }
  }

  pub fn initialize(&mut self, dir_name: &str) -> (Option<LogChunk>, u64) {
    let mut last_chunk : Option<LogChunk> = None;
    let mut highest_chunk_id : u32        = 0;
    let mut next_id    : u64              = 0;
    let re                          = Regex::new(r"chunks/(\d+)\.chk").unwrap();

    // Create chunks folder if not already there...
    fs::create_dir(dir_name).ok();

    // Read all files from directory in array and sort case insensitive by filename chunk id.
    let mut all_chunks = Vec::new();
    for entry in fs::read_dir(dir_name).unwrap() {
      all_chunks.push(entry);
    }
    all_chunks.sort_by(|a, b| {
      let chunk_file_a = a.as_ref().unwrap();
      let chunk_path_a   = chunk_file_a.path();
      let chunk_path_str_a = chunk_path_a.to_str().unwrap();
      let captures_a    = re.captures(chunk_path_str_a).unwrap();
      let chunk_id_a        = captures_a.get(1).unwrap().as_str().parse::<u32>().unwrap();

      let chunk_file_b = b.as_ref().unwrap();
      let chunk_path_b   = chunk_file_b.path();
      let chunk_path_str_b = chunk_path_b.to_str().unwrap();
      let captures_b   = re.captures(chunk_path_str_b).unwrap();
      let chunk_id_b       = captures_b.get(1).unwrap().as_str().parse::<u32>().unwrap();

      chunk_id_a.cmp(&chunk_id_b)
    });

    // Enumerate sorted log chunk files and load into index
    for entry in all_chunks.iter() {
      let chunk_file = entry.as_ref().unwrap();
      let chunk_path  = chunk_file.path();
      let chunk_path_str= chunk_path.to_str().unwrap();
      let captures   = re.captures(chunk_path_str).unwrap();
      let chunk_id       = captures.get(1).unwrap().as_str().parse::<u32>().unwrap();

      println!("Indexing: {:?} {:?}", chunk_file.path(), chunk_id);

      let (log_chunk, event_info) = LogChunk::index(chunk_id, chunk_path_str);

      for (i, (stream_name, _)) in event_info.iter().enumerate() {
        let index_element = IndexElement{
          chunk_number: log_chunk.id,
          offset : *log_chunk.offsets.get(i).unwrap()
        };

        self.add(stream_name, index_element);
      }

      // Files come in random order so keep track of highest chunk id so we end up knowing
      // last chunk (write chunk) and next_id
      if chunk_id > highest_chunk_id {
        highest_chunk_id = chunk_id;

        last_chunk       = Some(log_chunk);
        if event_info.len() > 0 {
          next_id        = (event_info.get(event_info.len()-1).unwrap().1)+1;
        } else {
          next_id        = 1;
        }
      }
    }
    //println!("{:?}, {}", last_chunk.as_ref().unwrap().id, next_id);
    (last_chunk, next_id)
  }

  pub fn add(&mut self, stream_name: &str, value: IndexElement) {
    println!("Found stream {:?}", stream_name);

    let value_copy = value.clone();

    // Target specified stream first
    if self.map.contains_key(stream_name) {
      let vector = self.map.get_mut(stream_name).unwrap();
      vector.push(value);
    } else {
      println!("Created {} stream", stream_name);
      // First time creating a new stream
      let value = vec![value];
      self.map.insert(stream_name.to_string(), value);
    }

    // Everything also always gets put in the $all stream...
    if self.map.contains_key("$all") {
      let vector = self.map.get_mut("$all").unwrap();
      vector.push(value_copy);
    } else {
      println!("Created $all stream");
      let value = vec![value_copy];
      self.map.insert("$all".to_string(), value);
    }
  }

  pub fn fetch_one(&mut self, stream_name: &str) -> &Vec<IndexElement> {
    if self.map.contains_key(stream_name) == false {
      println!("Adding empty stream {}!", stream_name);
      let value : Vec<IndexElement> = vec![];
      self.map.insert(stream_name.to_string(), value);
    }

    self.map.get(stream_name).unwrap()
  }
}