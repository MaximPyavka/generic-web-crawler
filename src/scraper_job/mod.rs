mod scraper_iterator;
use scraper_iterator::ScraperIterator;

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::convert::TryFrom;

use reqwest::{header, Client, Error, RequestBuilder};

use serde::Deserialize;
use tokio::runtime::Handle;
use tokio::task::block_in_place;
use url::Url;

use crate::auth::{AuthJob, AuthResult};
use crate::client_config::APP_USER_AGENT;
use crate::custom_types::PinnedFutureSender;
use crate::errors::ProcessorError;
use crate::headers::de_headers;
use crate::parser::{
    FinishedProcessingResult, NextProcessingStep, ProcessingResultUnit,
};
use crate::response_adaptor::{Resp, RespAdaptMarker};

use futures::{stream, StreamExt};

#[derive(Debug, Deserialize, Clone)]
pub enum HTTPMethod {
    GET,
    POST,
}

#[derive(Debug, Deserialize, Clone)]
pub enum HTTPParameterType {
    Name(String),
    Suffix(String),
}

#[derive(Debug, Deserialize, Clone)]
pub enum DynamicParameters {
    IntRange {
        name: HTTPParameterType,
        start: u16,
        end: u16,
        step: u16,
    },
    KeyWords {
        name: HTTPParameterType,
        words: Vec<String>,
    },
}

pub struct DynParamsIterator {
    iterator: Box<dyn ExactSizeIterator<Item = String> + Send>,
    name: HTTPParameterType,
}

impl DynParamsIterator {
    pub fn new(dyn_params: &DynamicParameters) -> Self {
        match dyn_params {
            DynamicParameters::IntRange {
                name,
                start,
                end,
                step,
            } => {
                let string_iterator = (*start..=*end)
                    .step_by(*step as usize)
                    .map(|i| i.to_string());
                DynParamsIterator {
                    name: name.clone(),
                    iterator: Box::new(string_iterator),
                }
            }
            DynamicParameters::KeyWords { name, words } => {
                let iterator: Box<dyn ExactSizeIterator<Item = String> + Send> =
                    Box::new(words.clone().into_iter());
                DynParamsIterator {
                    name: name.clone(),
                    iterator,
                }
            }
        }
    }

    pub fn len(&self) -> usize {
        self.iterator.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Iterator for DynParamsIterator {
    type Item = String;
    fn next(&mut self) -> Option<String> {
        self.iterator.next()
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct PlainScraperJob {
    #[serde(default)]
    default_parameters: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dynamic_parameters: Option<DynamicParameters>,
    authentication: Option<AuthJob>,
    #[serde(deserialize_with = "de_headers")]
    #[serde(default)]
    headers: header::HeaderMap,
    #[serde(default)]
    targets: BTreeMap<RespAdaptMarker, Vec<NextProcessingStep>>,
}

impl TryFrom<PlainScraperJob> for ScraperJob {
    type Error = String;

    fn try_from(scraper_job: PlainScraperJob) -> Result<Self, Self::Error> {
        let mut client = Client::builder()
        .cookie_store(true)
        .user_agent(APP_USER_AGENT)
        .default_headers(scraper_job.headers)
        .build().expect("Failed to build client in Move");

        if let Some(mut auth) = scraper_job.authentication {
                let auth_result = block_in_place(move || {
                    Handle::current().block_on(async move { auth.authenticate(client).await })
                });
    
                match auth_result {
                    Ok(auth) => match auth {
                        AuthResult::ClientSession(auth_client) => client = auth_client
                    },
                    Err(e) => {
                        eprintln!("FAILED TO AUTH {:?}", e);
                        panic!("FAIL");
                    }
                };
        } else {}

        Ok(ScraperJob {
            client,
            default_parameters: scraper_job.default_parameters,
            dynamic_parameters: scraper_job.dynamic_parameters,
            targets: scraper_job.targets,
        })
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(try_from = "PlainScraperJob")]
pub struct ScraperJob {
    #[serde(default)]
    default_parameters: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dynamic_parameters: Option<DynamicParameters>,
    #[serde(skip_deserializing)]
    client: Client,
    #[serde(default)]
    targets: BTreeMap<RespAdaptMarker, Vec<NextProcessingStep>>,
}

impl ScraperJob {

    fn new_request_with_url(&self, custom_url: Url) -> RequestBuilder {
        self.client.get(custom_url)
    }

    pub fn new_url_with_defaults(&self, url: &Url) -> Url {
        let mut new_url = url.clone();
        self.default_parameters.iter().for_each(|(name, value)| {
            new_url.query_pairs_mut().append_pair(name, value);
        });
        new_url
    }

    pub fn iter(self: &ScraperJob) -> ScraperIterator {
        let dyn_params_iterator = self.dynamic_parameters.as_ref().map(DynParamsIterator::new);
        ScraperIterator::new(dyn_params_iterator, self)
    }

    pub async fn run(self, url: Url, sender: PinnedFutureSender) {
        let url_ref = &url;
        let scraper_ref = &self;
        let sender_ref = &sender;
        stream::iter(scraper_ref.iter().with_url(url_ref))
            .for_each_concurrent(15, |resp| async move {
                let resp = resp
                    .send()
                    .await
                    .expect("FAILED TO GET RESPONSE FROM URL: {url:?}");
                let mut targets_iter = scraper_ref.targets.iter();
                let proc = targets_iter.next();

                // Assert that only one type of next step was in pipeline
                assert!(targets_iter.len() == 0);

                if let Some((marker, steps)) = proc {
                        let adopted_bytes_response_res = Resp::adopt(marker, resp).await;
                        let handled_response = scraper_ref
                            .handle_adopted_response_res(
                                adopted_bytes_response_res,
                                steps,
                                sender_ref,
                            )
                            .await;
                        if marker.is_bytes() {
                            if let Some(response_copy) = &handled_response {
                                stream::iter(targets_iter)
                                    .for_each_concurrent(15, |(marker, steps)| async move {
                                        let nxt_adoption_stage = response_copy.from_bytes(marker);
                                        scraper_ref
                                            .handle_adopted_response_res(
                                                Ok(nxt_adoption_stage),
                                                steps,
                                                sender_ref,
                                            )
                                            .await;
                                    })
                                    .await;
                                }
                        }
                } else {}
            })
            .await;
    }

    pub async fn handle_adopted_response_res(
        &self,
        adopted_response_res: Result<Resp, Error>,
        steps: &[NextProcessingStep],
        sender: &PinnedFutureSender,
    ) -> Option<Resp> {
        if adopted_response_res.is_err() {
            eprintln!(
                "{}",
                ProcessorError::from(adopted_response_res.unwrap_err())
            );
            None
        } else {
            let adopted_response = adopted_response_res.unwrap();
            self.process_adopted_response(&adopted_response, steps, sender)
                .await;
            Some(adopted_response)
        }
    }

    pub async fn process_adopted_response(
        &self,
        adopted_response: &Resp,
        steps: &[NextProcessingStep],
        sender: &PinnedFutureSender,
    ) {
        for step in steps {
            match step {
                NextProcessingStep::Store(storage) => {
                    storage.store(adopted_response).await;
                }
                NextProcessingStep::Process(proc) => {
                    let step_result = proc.process(adopted_response);
                    match step_result {
                        Ok(ref results) => match results {
                            FinishedProcessingResult::VectorResult(result_vector) => {
                                proc.next_steps().iter().for_each(|proc_step| {
                                    result_vector.iter().for_each(|proc_result| {
                                        self.process_processed_result(
                                            proc_result,
                                            proc_step,
                                            sender,
                                        )
                                    });
                                });
                            }
                            FinishedProcessingResult::NothingRequired => {}
                        },
                        Err(e) => {
                            println!("ERROR TRYING TO HANDLE {:?}", e);
                            // todo!("HANDLE PROCESSING ERROR")
                        }
                    }
                }
                NextProcessingStep::Scrape(_) => {
                    unreachable!("Cannot run Scrape as handler for ScraperJob result.")
                }
            }
        }
    }

    pub fn process_processed_result(
        &self,
        proc_result: &ProcessingResultUnit,
        next_proc_step: &NextProcessingStep,
        sender: &PinnedFutureSender,
    ) {
        match (proc_result, next_proc_step) {
            (ProcessingResultUnit::URL(url), NextProcessingStep::Scrape(scraper)) => {
                self.spawn_new_scraper_from_url(scraper, sender, url);
            }
            (ProcessingResultUnit::Str(text), NextProcessingStep::Process(proc)) => {
                match proc.process_string_result(text) {
                    Ok(proc_result) => match proc_result {
                        FinishedProcessingResult::VectorResult(results) => {
                            results.iter().for_each(|res| {
                                proc.next_steps().iter().for_each(|next_step| {
                                    self.process_processed_result(res, next_step, sender);
                                });
                            })
                        }
                        FinishedProcessingResult::NothingRequired => {}
                    },
                    Err(_) => {
                        todo!("LOGGING INNER");
                    }
                }
            }
            (proc_unit, next_step) => {
                unreachable!("Cannot process {:?} & {:?}", proc_unit, next_step);
            }
        };
    }

    pub fn spawn_new_scraper_from_url(
        &self,
        next_scraper_job: &ScraperJob,
        sender: &PinnedFutureSender,
        url: &Url,
    ) {
        let sender_clone = sender.clone();
        let url_clone = url.clone();
        let mut new_job = next_scraper_job.clone();
        new_job.client = self.client.clone();

        tokio::spawn(async move {
            if sender_clone
                .send(Box::pin(new_job.run(url_clone, sender_clone.clone())))
                .await.is_err() {
                    eprintln!("Failed to start new Scraperjob.")
                };
        });
    }
}