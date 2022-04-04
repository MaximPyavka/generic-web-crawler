use std::ops::RangeInclusive;

use reqwest::{RequestBuilder};
use url::Url;

use super::{DynParamsIterator, ScraperJob, HTTPParameterType};

pub struct ScraperIterator<'a> {
    dyn_params: Option<DynParamsIterator>,
    scraper: &'a ScraperJob,
    counter: RangeInclusive<usize>,
    url: Option<Url>,
}

impl<'a> ScraperIterator<'a> {
    pub fn new(dyn_params: Option<DynParamsIterator>, scraper: &ScraperJob) -> ScraperIterator {
        let end_bound = match &dyn_params {
            Some(params) => params.len(),
            None => 1,
        };

        ScraperIterator {
            dyn_params,
            scraper,
            counter: 1..=end_bound,
            url: None,
        }
    }

    pub fn with_url(mut self, url: &Url) -> Self {
        self.url = Some(url.clone());
        self
    }
}

impl<'a> Iterator for ScraperIterator<'a> {
    type Item = RequestBuilder;

    fn next(&mut self) -> Option<Self::Item> {
        self.counter.next().and_then(|_| {
            let url = &self.url;
            let params = &mut self.dyn_params;
            let scraper = &self.scraper;
            url.as_ref().map(|just_url| {
                let mut url_with_defaults = scraper.new_url_with_defaults(just_url);
                match params {
                    Some(dyn_params_iterator) => {
                        // let self_ref = &self;

                        let param_type = dyn_params_iterator.name.clone();
                        let param_value = dyn_params_iterator
                            .next()
                            .expect("Unexpected value of iterator");
                        match param_type {
                            HTTPParameterType::Name(param_name) => {
                                url_with_defaults
                                    .query_pairs_mut()
                                    .append_pair(&param_name, &param_value);
                            }
                            HTTPParameterType::Suffix(suff) => {
                                let new_path =
                                    &format!("{}{}{}", url_with_defaults.path(), suff, param_value);
                                url_with_defaults.set_path(new_path);
                                println!("NEW URL {}", url_with_defaults);
                            }
                        };
                    }
                    None => (),
                }
                scraper.new_request_with_url(url_with_defaults)
            })
        })
    }
}