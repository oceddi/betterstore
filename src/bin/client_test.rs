use tonic::transport::Endpoint;
use tonic::Request;

use betterstore::api;
use api::events_client::EventsClient;
use api::{AppendToStreamRequest, ReadStreamRequest};
use rand::Rng;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = Endpoint::from_static("http://127.0.0.1:50051");
    
    let mut client  = EventsClient::connect(addr).await?;
    let mut rng = rand::thread_rng();
    let stream_names = vec!["test1", "test2", "test3"];

    for i in 0..1 {
        let stream_name = stream_names[rng.gen_range(0..3)];

        let mut event_vec = vec![];
        for j in 0..10 {
            event_vec.push( format!("{} remote payload {} {}", stream_name, i, j));
        }
        let request = Request::new(AppendToStreamRequest{
            stream_name : stream_name.to_string(),
            events      : event_vec
        });
        let _ = client.append_to_stream(request).await?;
    }

    let request = Request::new(
        ReadStreamRequest{
            stream_name: "test1".to_string(),
            stream_position: 5
        }
    );
    let mut response = client.read_stream(request).await?.into_inner();


    while let Some(res) = response.message().await? {
        println!("response: {:?}", res);
    }

    let request = Request::new(
        ReadStreamRequest{
            stream_name: "test2".to_string(),
            stream_position: 5
        }
    );
    let mut response = client.read_stream(request).await?.into_inner();


    while let Some(res) = response.message().await? {
        println!("response: {:?}", res);
    }

    let request = Request::new(
        ReadStreamRequest{
            stream_name: "test3".to_string(),
            stream_position: 5
        }
    );
    let mut response = client.read_stream(request).await?.into_inner();


    while let Some(res) = response.message().await? {
        println!("response: {:?}", res);
    }

    Ok(())
}