use std::io::{self, Write};
use chrono::prelude::*;
use tonic::transport::Endpoint;
use tonic::Request;
use betterstore::api;
use api::events_client::EventsClient;
use api::{AppendToStreamRequest, ReadStreamRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let addr = Endpoint::from_static("http://127.0.0.1:50051");
  let mut client  = EventsClient::connect(addr).await?;
  let now = Local::now();
  let stream_name = format!("Conversation on {} at {}.", now.format("%Y-%m-%d"), now.format("%H:%M:%S"));

  loop {
    let mut input = String::new();

    print!("Enter a message (or press Ctrl-D to quit): ");
    io::stdout().flush().expect("Failed to flush stdout");

    match io::stdin().read_line(&mut input) {
      Ok(0) => break,
      Ok(_) => {
        let request = Request::new(AppendToStreamRequest{
          stream_name : stream_name.to_string(),
          events      : vec![input.trim().to_string()]
        });
        let _ = client.append_to_stream(request).await?;
      }
      Err(error) => {
        eprintln!("Error reading line: {}", error);
        break;
      }
    }
  }

  println!("\n\nHere is what we talked about:");

  let request = Request::new(
    ReadStreamRequest{
        stream_name: stream_name.to_string(),
        stream_position: 0
    }
  );
  let mut response = client.read_stream(request).await?.into_inner();
  while let Some(res) = response.message().await? {
    println!("{:?}", res);
  }
    Ok(())
}
