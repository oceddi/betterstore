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
      let request = request.into_inner();

      let request = AppendToStream{
        stream_name : request.stream_name,
        events      : request.events
      };

      match self.actor_addr.send(request).await {
        Ok(Ok(_)) => {
          let response = AppendToStreamResponse {
            response : "success".to_string()
          };
          Ok(Response::new(response))
        }
        Ok(Err(e)) => Err(Status::invalid_argument(e)),
        Err(e) => Err(Status::internal(e.to_string()))
      }
  }

  // ReadStream
  type ReadStreamStream = ReceiverStream<Result<ReadStreamResponse, Status>>;

  async fn read_stream(&self, request: Request<ReadStreamRequest>)
    -> Result<Response<Self::ReadStreamStream>, Status> {
      let ( tx, rx) = mpsc::channel(4);

      let response = ReadStream{
        stream_name : request.get_ref().stream_name.clone(),
        stream_position : request.get_ref().stream_position,
        tx_channel  : tx
      };

      self.actor_addr.send(response).await
        .map_err(|e| Status::internal(e.to_string()))?
        .map_err(|_| Status::internal("Failed to start read"))?;

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
