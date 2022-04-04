use std::fmt::Debug;


use serde::Deserialize;

use url::Url;
use futures::{stream, StreamExt};

use crate::custom_types::PinnedFutureSender;

use crate::scraper_job::ScraperJob;


#[derive(Debug, Deserialize)]
pub struct ScraperUnit {
    scraper: ScraperJob,
    urls: Vec<Url>,
}

impl ScraperUnit {
    pub async fn run(self, sender: PinnedFutureSender) {
        let ref_sender = &sender;
        let scraper = &(self.scraper);
        stream::iter(self.urls.into_iter())
            .for_each_concurrent(2, |url| async move {
                scraper.clone().run(url.clone(), ref_sender.clone()).await
            })
            .await
    }
}
