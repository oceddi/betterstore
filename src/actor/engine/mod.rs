use std::borrow::BorrowMut;
use super::super::api::ReadStreamResponse;
use tokio::sync::mpsc::Sender;
use tonic::Status;

use index::Index;
use writer::Writer;
use reader::ReaderStream;

pub mod event;
mod chunk;
mod index;
mod writer;
mod reader;

pub struct Engine {
  index   : Index,
  writer  : Writer
}

impl Engine {
  pub fn new() -> Self {

    let mut index                       = Index::new();
    let (wchunk, next_id) = index.initialize("chunks");

    let writer = Writer::new(wchunk, next_id);

    Self {
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
        Ok(self.writer.append_events(
          self.index.borrow_mut(),
          stream_name,
          events
        ))
      }
    }
  }

  pub async fn read_stream(&mut self, stream_name: String, stream_position: u64, tx_channel: Sender<Result<ReadStreamResponse, Status>>) {
    let mut reader = ReaderStream::new(stream_name, &mut self.index);

    reader.read_stream(tx_channel, stream_position).await;
  }

}