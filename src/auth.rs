use std::collections::HashMap;
use std::fmt::Debug;

use async_recursion::async_recursion;
use reqwest::{header, Client};
use serde::Deserialize;
use url::Url;

use crate::errors::ProcessorError;
use crate::scraper_job::HTTPMethod;
use crate::parser::{FinishedProcessingResult, ProcessingResultUnit, ProcessingStep};
use crate::headers::de_headers;


#[derive(Debug, Deserialize, Clone)]
pub enum AuthResultJSON {
    ClientSession,
    NextStep(Box<AuthJob>),
}

pub enum AuthResult {
    ClientSession(Client),
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthJob {
    #[serde(default)]
    #[serde(deserialize_with = "de_headers")]
    headers: header::HeaderMap,
    request_url: Url,
    http_method: HTTPMethod,
    #[serde(default)]
    request_form: HashMap<String, String>,
    proc_step: Option<ProcessingStep>,
    action: AuthResultJSON,
}

impl AuthJob {
    #[async_recursion]
    pub async fn authenticate(&mut self, client: Client) -> Result<AuthResult, ProcessorError> {
        let mut auth_call = match self.http_method {
            HTTPMethod::GET => client.get(self.request_url.clone()),
            HTTPMethod::POST => client
                .post(self.request_url.clone())
                .form(&self.request_form)
        };
        auth_call = auth_call.headers(self.headers.clone());

        let auth_resp = auth_call.send().await?;
        let resp_status = auth_resp.status();
        let proc_result = match &self.proc_step {
            Some(proc) => {
                let resp_text = auth_resp.text().await?;
                // write("kokoko.html", &resp_text);

                match proc.process_string_result(&resp_text) {
                    Ok(proc_res) => match proc_res {
                        FinishedProcessingResult::VectorResult(res) => Some(res),
                        FinishedProcessingResult::NothingRequired => None,
                    },
                    Err(e) => panic!("FAILED TO AUTH {:?}", e),
                }
            }
            None => None,
        };
        if resp_status.is_success() {
            match &mut self.action {
                AuthResultJSON::ClientSession => Ok(AuthResult::ClientSession(client)),
                AuthResultJSON::NextStep(ref mut step) => {
                    let as_bl = async move {
                        step.inherit(proc_result);
                        step.authenticate(client).await
                    };
                    as_bl.await
                }
            }
        } else {
            Err(ProcessorError::AuthenticationError(
                "Cannot Authenticate".to_string(),
            ))
        }
    }

    pub fn inherit(&mut self, maybe_proc_results: Option<Vec<ProcessingResultUnit>>) {
        maybe_proc_results.into_iter().for_each(|proc_results| {
            proc_results.into_iter().for_each(|proc_res| {
                match proc_res {
                    ProcessingResultUnit::FormParameter { name, value } => {
                        self.request_form.insert(name, value)
                    }
                    _ => unimplemented!("Unexpected processing result in Auth step"),
                };
            })
        });
    }
}
