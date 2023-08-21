use tonic::transport::Endpoint;
use tonic::Request;

use betterstore::api;
use api::events_client::EventsClient;
use api::ReadStreamRequest;

/* Design Ideas: */
/* Initialization:      */
/*    Set of rows that consist of: */
/*      Fact,  Query to Find Answer,  Confidence/Trust Score  */
/*      My Name is Bob,   (query to lookup my name),  0 - 1.0 */
/*

    During startup, run initialization.
      For each row, run the query to see if there is a result/fact with high confidence score (> 0.8).
      If not, ask the question to the trainer and record the answer with confidence of 1.0
      Once all queries have facts with confidence > 0.8 then leave init step.

      This should allow one to extend the init parameters over time.
      These core facts might be eroded due to new facts (which would lower their confidence score, causing a reinit of ground truth).

      You name is Jeff (lowers confidence in name=Bob if source is trusted)

 */

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let addr = Endpoint::from_static("http://127.0.0.1:50051");

  let mut client  = EventsClient::connect(addr).await?;

  // Start for beginning
  let mut next_pos : u64 = 0;

  loop {
    let request = Request::new(
      ReadStreamRequest{
        stream_name: "$all".to_string(),
        stream_position: next_pos
      }
    );
    let mut response = client.read_stream(request).await?.into_inner();
    while let Some(res) = response.message().await? {

      let words: Vec<String> = res.event.split_whitespace().map(|s| s.to_string()).collect();

      println!("words in sentence: {:?}", words);
      let article_indices = identify_articles(&words);
      println!("articles: {:?}", article_indices);

      let adjective_indices = identify_adjectives(&words);
      println!("adjectives: {:?}", adjective_indices);

      next_pos = res.stream_position + 1;
    }
  }
  //Ok(())
}

fn identify_articles(sentence: &Vec<String>) -> Vec<usize> {
  let articles = vec!["a", "an", "the"]; // list of articles
  let mut result = Vec::new();

  for (i, word) in sentence.iter().enumerate() {
      if articles.contains(&word.to_lowercase().as_str()) { // check if the word is an article
          result.push(i); // add the index of the article to the result vector
      }
  }

  return result;
}

fn identify_adjectives(sentence: &Vec<String>) -> Vec<String> {
  let mut adjectives = Vec::new();

  for (index, word) in sentence.iter().enumerate() {
      let word_lc = word.to_lowercase();

      if word_lc == "a" || word_lc == "an" || word_lc == "the" {
          continue;
      }

      if word.chars().all(|c| c.is_alphabetic()) {
          if index + 1 < sentence.len() {
              let next_word = &sentence[index + 1];
              if next_word.chars().all(|c| c.is_alphabetic()) {
                  adjectives.push(word.clone());
              }
          }
      }
  }

  adjectives
}
