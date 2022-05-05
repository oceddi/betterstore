use tonic::{Request, Response, Status};
use tonic::transport::Server;
use actix::{Addr, Actor};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use betterstore::api::{self, ReadStreamRequest, ReadStreamResponse};
use betterstore::actor::{BetterStoreActor, AppendToStream, ReadStream};

use api::events_server::EventsServer;
use api::events_server::{Events};
use api::{AppendToStreamRequest, AppendToStreamResponse};

// Defining a struct for our RPC service
pub struct Api {
  actor_addr : Addr<BetterStoreActor>
}

impl Api {
  pub fn new(actor_addr : Addr<BetterStoreActor>) -> Self {
    Self { actor_addr }
  }
}

#[tonic::async_trait]
impl Events for Api {

  // AppendToStream
  async fn append_to_stream(&self,  request: Request<AppendToStreamRequest>) 
    -> Result<Response<AppendToStreamResponse>, Status> {
      let mut events = vec!["".to_string(); request.get_ref().events.len()];

      // This is to setup ownership lifetime, both vectors must be equal size.
      events.clone_from_slice(&request.get_ref().events);

      let request = AppendToStream{
        stream_name : request.get_ref().stream_name.clone(),
        events      : events
      };
      let _ = self.actor_addr.send(request).await;

      let response = AppendToStreamResponse {
        response : "success".to_string()
      };
      Ok(Response::new(response))
  }

  // ReadStream
  type ReadStreamStream = ReceiverStream<Result<ReadStreamResponse, Status>>;

  async fn read_stream(&self, request: Request<ReadStreamRequest>)
    -> Result<Response<Self::ReadStreamStream>, Status> {
      let ( tx, rx) = mpsc::channel(4);

      let request = ReadStream{
        stream_name : request.get_ref().stream_name.clone(),
        stream_position : request.get_ref().stream_position,
        tx_channel  : tx
      };

      let _ = self.actor_addr.send(request).await;

      Ok(Response::new(ReceiverStream::new(rx)))
    }
}


#[actix::main] 
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "0.0.0.0:50051".parse()?;

    println!("Starting betterstore server...");

    // Our RPC API
    let better_store_actor = BetterStoreActor::new().start();
    let api = Api{actor_addr: better_store_actor};

    // Start RPC server defined in server.rs
    Server::builder()
        .add_service(EventsServer::new(api))
        .serve(addr)
        .await?;

    Ok(())
}
