pub mod auth;
pub mod client_config;
pub mod custom_types;
pub mod errors;
pub mod scraper_unit;
pub mod headers;
pub mod logging;
pub mod parser;
pub mod response_adaptor;
pub mod storage;
pub mod scraper_job;

use std::pin::Pin;
use std::fs;

use futures::{Future, StreamExt};

use scraper_unit::ScraperUnit;
use tokio::sync::mpsc::channel;
use tokio_stream::wrappers::ReceiverStream;

pub fn new_dummy_scraper() -> ScraperUnit {
    let raw = fs::read_to_string("./json_templates/olx.json").unwrap();
    serde_json::from_str(&raw).unwrap()
}


#[tokio::main]
pub async fn main() {
    let (tx, rx) = channel::<Pin<Box<(dyn Future<Output = ()> + Send)>>>(15);

    tokio::spawn(async move {
        if tx.send(Box::pin(new_dummy_scraper().run(tx.clone()))).await.is_err() {
             eprintln!("Failed to push initial Scaper Unit to the channel.");
        }
    });

    let stream: ReceiverStream<_> = rx.into();

    stream.for_each_concurrent(15, |fut| async {
        fut.await;
    }).await;
}