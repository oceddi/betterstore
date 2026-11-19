use actix::{Actor, Context, Handler, Message, AsyncContext, fut::{wrap_future}};
use self::engine::Engine;
use super::api::ReadStreamResponse;
use tokio::sync::{mpsc::Sender};
use tonic::Status;

// Pull in modules defined in subdirs below
pub mod engine;

// Define Actor Messages
#[derive(Message, Debug)]
#[rtype(result = "Result<u64, String>")]
pub struct AppendToStream {
  pub stream_name : String,
  pub events      : Vec<String>
}

#[derive(Message, Debug)]
#[rtype(result = "Result<(), ()>")]
pub struct ReadStream {
  pub stream_name : String,
  pub tx_channel  : Sender<Result<ReadStreamResponse, Status>>,
  pub stream_position: u64
}

// Define Actor Execution Context in this struct.
pub struct BetterStoreActor {
  engine : Engine
}

impl BetterStoreActor {
  pub fn new() -> Self {
    Self::with_dir("chunks")
  }

  pub fn with_dir(dir: &str) -> Self {
    Self {
      engine : Engine::new(dir)
    }
  }
}

impl Actor for BetterStoreActor {
  type Context = Context<Self>;

  fn started(&mut self, _ctx: &mut Context<Self>) {
    println!("BetterStoreActor is started!");
  }

  fn stopped(&mut self, _ctx: &mut Context<Self>) {
    println!("BetterStoreActor is stopped!");
  }
}

impl Handler<AppendToStream> for BetterStoreActor {
  type Result = Result<u64, String>;

  fn handle(&mut self, msg: AppendToStream, _ctx: &mut Context<Self>) -> Result<u64, String> {
    self.engine.append_events(
      msg.stream_name,
      msg.events
    )
  }
}

impl Handler<ReadStream> for BetterStoreActor {
  type Result = Result<(), ()>;

  fn handle(&mut self, msg: ReadStream, ctx: &mut Context<Self>) -> Self::Result {
    // Read synchronously so no engine state is borrowed across an await.  Sending is
    // then done in a spawned future, so a slow client only stalls its own stream and
    // never blocks other messages to this actor.
    let responses = self.engine.read_stream(&msg.stream_name, msg.stream_position);
    let tx_channel = msg.tx_channel;

    let fut =
      async move {
        match responses {
          Ok(responses) => {
            for response in responses {
              // Receiver dropped means the client went away, so stop sending.
              if tx_channel.send(Ok(response)).await.is_err() {
                break;
              }
            }
          }
          Err(e) => {
            let _ = tx_channel.send(Err(Status::internal(e.to_string()))).await;
          }
        }
    };

    ctx.spawn(wrap_future(fut));
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use std::time::Duration;
  use actix::Actor;
  use tokio::sync::mpsc;
  use super::*;
  use crate::test_util::temp_dir;

  fn append(stream_name: &str, count: usize) -> AppendToStream {
    AppendToStream {
      stream_name : stream_name.to_string(),
      events      : (0..count).map(|i| format!("event {}", i)).collect()
    }
  }

  #[actix::test]
  async fn slow_reader_does_not_block_appends() {
    let dir  = temp_dir("slow_reader");
    let addr = BetterStoreActor::with_dir(&dir).start();

    addr.send(append("s", 10)).await.unwrap().unwrap();

    // More events than the channel holds, and the receiver is never drained.
    let (tx, _rx) = mpsc::channel(4);
    addr.send(ReadStream{
      stream_name     : "s".to_string(),
      tx_channel      : tx,
      stream_position : 0
    }).await.unwrap().unwrap();

    let next_id = tokio::time::timeout(Duration::from_secs(5), addr.send(append("s", 1)))
      .await
      .expect("append blocked behind a slow reader")
      .unwrap()
      .unwrap();
    assert_eq!(next_id, 11);
  }
}
