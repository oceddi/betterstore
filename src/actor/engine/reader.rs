
use super::index::{Index, IndexElement};
use super::super::super::api::ReadStreamResponse;
use tokio::sync::mpsc::Sender;
use tonic::Status;

use super::chunk::LogChunk;


pub struct ReaderStream<'stream> {
  stream_name   : String,
  index_entries : &'stream Vec<IndexElement>
}

impl <'stream> ReaderStream<'stream> {
  pub fn new(stream_name: String, index: &'stream mut Index) -> Self {
    let index_entries = index.fetch_one(&stream_name);

    Self {
      stream_name,
      index_entries
    }
  }

  pub async fn read_stream(&mut self, tx_channel: Sender<Result<ReadStreamResponse, Status>>, start : u64) {

    if self.index_entries.len() == 0 {
      return;
    }

    if start > (self.index_entries.len()-1) as u64 {
      return;
    }

    let mut current = start as usize;
    loop {
      let index_element = &self.index_entries[current];
      let chunk_path_str = format!("chunks/{}.chk", index_element.chunk_number);
      println!("Reading from offset {}", index_element.offset);

      let chunk_events = LogChunk::stream_events_out(index_element.offset, chunk_path_str.as_str());
      let chunk_events = match chunk_events {
        Ok(ce) => ce,
        Err(error) => panic!("Problem opening chunk file: {:?}", error),
      };

      if self.stream_name == "$all" {
        for event in chunk_events.iter() {
          let response = ReadStreamResponse {
            event : event.payload.clone(),
            stream_position : event.id
          };

          // TODO: Handle send errors
          let _ = tx_channel.send(Ok(response)).await;
          current += 1;
        }
      } else {
        for event in chunk_events.iter().filter(|evn| evn.name == self.stream_name) {
          let response = ReadStreamResponse {
            event : event.payload.clone(),
            stream_position: event.id
          };

          // TODO: Handle send errors
          let _ = tx_channel.send(Ok(response)).await;
          current += 1;
        }
      }
      if current > self.index_entries.len()-1 {
        break;
      }
    }
  }
}