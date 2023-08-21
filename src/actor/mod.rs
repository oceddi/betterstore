use std::sync::{Arc, Mutex};

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
#[derive(Clone)]
pub struct BetterStoreActor {
  engine : Arc<Mutex<Engine>>
}

impl BetterStoreActor {
  pub fn new() -> Self {
    Self {
      engine : Arc::new(Mutex::new(Engine::new()))
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
    let engine = self.engine.clone();
    let mut engine = engine.lock().unwrap();
    engine.append_events(
      msg.stream_name,
      msg.events
    ) 
  }
}

impl Handler<ReadStream> for BetterStoreActor {
  type Result = Result<(), ()>;

  fn handle(&mut self, msg: ReadStream, ctx: &mut Context<Self>) -> Self::Result {
    let engine = self.engine.clone();

    let fut = 
      async move {
        let mut engine = engine.lock().unwrap();

        engine.read_stream(
          msg.stream_name,
          msg.stream_position,
          msg.tx_channel
        ).await
    };

    let fut = wrap_future(fut);

    ctx.spawn(fut);
    Ok(())
  }
  // Implementation with no Arc or Mutex guard.
  // NOTE: clone causes each clone to have different index state between read streams and write streams
  // fn handle(&mut self, msg: ReadStream, ctx: &mut Context<Self>) -> Self::Result {
  //   let mut this = self.clone();
  //   let fut = 
  //     async move {
  //       this.engine.read_stream(
  //         msg.stream_name,
  //         msg.stream_position,
  //         msg.tx_channel
  //       ).await
  //   };

  //   let fut = wrap_future(fut);

  //   ctx.spawn(fut);
  //   Ok(())
  // }
}