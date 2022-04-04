use std::result::Result;
use std::vec;


use regex::Regex;
use reqwest::Url;
use scraper::{Html, Selector};
use serde::{de, Deserialize, Deserializer};
use serde_json::{Value};



use crate::errors::ProcessorError;
use crate::scraper_job::ScraperJob;
use crate::response_adaptor::Resp;
use crate::storage::Storage;

pub type ProcessingResult = Result<FinishedProcessingResult, ProcessorError>;

pub enum FinishedProcessingResult {
    VectorResult(Vec<ProcessingResultUnit>),
    NothingRequired,
}

#[derive(Debug, Deserialize, Clone)]
pub enum JSONProcessingResultUnit {
    URL,
    PartialURL(Url),
    Base(String),
    Str,
    Parameter(Url, String),
    FormParameter(String),
}

#[derive(std::fmt::Debug)]
pub enum ProcessingResultUnit {
    URL(Url),
    Str(String),
    FormParameter {name: String, value: String},
}

#[derive(std::fmt::Debug, Deserialize)]
pub enum ParserType {
    Html(String),
    Regex(String),
    JSON(String),
}

#[derive(std::fmt::Debug, Deserialize, Clone)]
pub enum Capture {
    Many(usize),
    All,
}

#[derive(std::fmt::Debug, Deserialize, Clone)]
pub enum LookupBlock {
    Pos(usize),
    Att(String),
    Take,
}

#[derive(std::fmt::Debug, Deserialize, Clone)]
pub enum NextProcessingStep {
    Process(ProcessingStep),
    Scrape(ScraperJob),
    Store(Storage),
}

#[derive(std::fmt::Debug, Deserialize, Clone)]
pub enum SelectorTarget {
    Attr(String),
    Text,
}



#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum ProcessingStep {
    Html {
        #[serde(deserialize_with = "de_selector")]
        selector: Selector,
        capture_elements: Capture,
        selector_target: SelectorTarget,
        proc_result: JSONProcessingResultUnit,
        next_steps: Vec<NextProcessingStep>,
    },
    Regex {
        #[serde(deserialize_with = "de_regex")]
        regex: Regex,
        capture_elements: Capture,
        groups: Vec<u8>,
        next_steps: Vec<NextProcessingStep>,
        proc_result: JSONProcessingResultUnit,
    },
    JSON {
        lookup_search: Vec<LookupBlock>,
        next_steps: Vec<NextProcessingStep>,
        proc_result: JSONProcessingResultUnit,
    },
}

fn de_selector<'de, D>(deserializer: D) -> Result<Selector, D::Error>
where
    D: Deserializer<'de>,
{
    let raw_selector_string: String = Deserialize::deserialize(deserializer)?;
    Selector::parse(&raw_selector_string).map_err(|_| {
        de::Error::invalid_value(
            de::Unexpected::Str(&format!("Invalid CSS selector {:?}.", raw_selector_string)),
            &r#""CSS selector according to https://www.w3schools.com/cssref/css_selectors.asp .""#,
        )
    })
}

fn de_regex<'de, D>(deserializer: D) -> Result<Regex, D::Error>
where
    D: Deserializer<'de>,
{
    let raw_regex_string: String = Deserialize::deserialize(deserializer)?;
    Regex::new(&raw_regex_string).map_err(|_| {
        de::Error::invalid_value(
            de::Unexpected::Str(&format!("Invalid Regex expression {:?}.", raw_regex_string)),
            &r#""Valid regex expression.""#,
        )
    })
}

impl ProcessingStep {
    pub fn process(&self, resp: &Resp) -> ProcessingResult {
        match resp {
            Resp::RespText(text) => self.process_string_result(text),
            _ => { unreachable!("For now") }
        }
    }

    pub fn process_string_result(&self, text: &str) -> ProcessingResult {
        match self {
            ProcessingStep::Html {
                selector,
                capture_elements,
                selector_target,
                proc_result,
                next_steps: _,
            } => self.process_from_html(text, selector, capture_elements, selector_target, proc_result),
            ProcessingStep::Regex {
                regex,
                groups,
                capture_elements,
                proc_result,
                next_steps: _,
            } => self.process_from_regex(text, regex, groups, capture_elements, proc_result),
            ProcessingStep::JSON {
                lookup_search,
                next_steps: _,
                proc_result,
            } => self.process_from_json(text, lookup_search, proc_result)
        }
    }

    fn string_to_processing_result(
        &self,
        string_result: String,
        proc_result_unit: &JSONProcessingResultUnit,
    ) -> ProcessingResultUnit {
        match proc_result_unit {
            JSONProcessingResultUnit::PartialURL(_url) => {
                let new_url = _url.join(&string_result).unwrap_or_else(|_| panic!("FAILED to JOIN URL {:?} and {:?}",
                        _url.as_str(),
                        string_result.as_str()));
                ProcessingResultUnit::URL(new_url)
            }
            JSONProcessingResultUnit::Parameter(url, param_name) => {
                let mut new_url = url.clone();
                new_url
                    .query_pairs_mut()
                    .append_pair(param_name, string_result.as_str());
                ProcessingResultUnit::URL(new_url)
            }
            JSONProcessingResultUnit::URL => {
                println!("NEW URL {}", string_result);
                let new_url = Url::parse(string_result.as_str())
                    .unwrap_or_else(|_| panic!("Failed to create URL from {} ", string_result));
                ProcessingResultUnit::URL(new_url)
            },
            JSONProcessingResultUnit::FormParameter(name) => {
                ProcessingResultUnit::FormParameter {name: name.clone(), value: string_result}
            },
            JSONProcessingResultUnit::Base(url_base) => {
                let new_url = url_base.clone() + &string_result;
                ProcessingResultUnit::URL(Url::parse(&new_url).unwrap())
            },
            _ => ProcessingResultUnit::Str(string_result),
        }
    }

    pub fn process_from_html(
        &self,
        html: &str,
        selector: &Selector,
        capture_elements: &Capture,
        selector_target: &SelectorTarget,
        proc_result_unit: &JSONProcessingResultUnit,
    ) -> ProcessingResult {
        let document_tree = Html::parse_document(html);
        // TODO logging
        let selected = document_tree
            .select(selector);
            
        let mapped = match capture_elements {
            Capture::Many(n) => selected.take(*n),
            Capture::All => selected.take(1_000_000)
        };

        let vec_parsed = mapped
            .map(|selected| match &selector_target {
                SelectorTarget::Attr(attr) => {
                    if let Some(selected_value) = selected.value().attr(attr) {
                        Ok(selected_value.to_owned())
                    } else {
                        Err("Failed to get required attirubutes")
                    }
                },
                SelectorTarget::Text => Ok(selected.text().collect::<String>()),
            })
            .filter_map(|selected_text| selected_text.ok())
            .map(|str_element| self.string_to_processing_result(str_element, proc_result_unit))
            .collect::<Vec<ProcessingResultUnit>>();

        if vec_parsed.is_empty() {
            Err(ProcessorError::NothingToCaptureError)
        } else {
            Ok(FinishedProcessingResult::VectorResult(vec_parsed))
        }
    }

    pub fn process_from_regex(
        &self,
        text: &str,
        regex: &Regex,
        groups: &[u8],
        capture: &Capture,
        proc_result_unit: &JSONProcessingResultUnit,
    ) -> ProcessingResult {
        let mut captures = regex
            .captures_iter(text)
            .into_iter()
            .filter_map(|capt| -> Option<(&u8, &str)>  {
                groups
                    .iter()
                    .find_map(|gr| capt.get(*gr as usize).map(|res| (gr, res.as_str())))
            })
            .collect::<Vec<(&u8, &str)>>();

        captures.sort_by_key(|(pos, _)| {
                *pos
            });

        let capture_result = captures.iter().take(1)
            .map(|(_, str_element)| {
                self.string_to_processing_result(str_element.to_string(), proc_result_unit)
            });

        let capt_vector = match capture {
            Capture::Many(n) => capture_result.take(*n).collect::<Vec<ProcessingResultUnit>>(),
            Capture::All => capture_result.collect::<Vec<ProcessingResultUnit>>(),
        };

        if capt_vector.is_empty() {
            Err(ProcessorError::NothingToCaptureError)
        } else {
            Ok(FinishedProcessingResult::VectorResult(capt_vector))
        }
    }

    pub fn process_from_json(
        &self,
        text: &str,
        lookup_search: &[LookupBlock],
        proc_result_unit: &JSONProcessingResultUnit,
    ) -> ProcessingResult {
        let mut vec_parsed = vec![];
        let json_object: Value = serde_json::from_str(text).unwrap();
        self.parse_json_value(0, &mut vec_parsed, &json_object, lookup_search);
        let vec_results: Vec<ProcessingResultUnit> = vec_parsed
            .into_iter()
            .map(|str_element| self.string_to_processing_result(str_element, proc_result_unit))
            .collect();

        if vec_results.is_empty() {
            Err(ProcessorError::NothingToCaptureError)
        } else {
            Ok(FinishedProcessingResult::VectorResult(vec_results))
        }
    }

    pub fn parse_json_value(
        &self,
        cur_lookup: usize,
        results_vec: &mut Vec<String>,
        value: &Value,
        lookup_search: &[LookupBlock],
    ) {
        // Todo handle Panic + lookup overflow
        match (&lookup_search[cur_lookup], value) {
            (LookupBlock::Att(attr), Value::Object(sub_val)) => {
                let new_value = sub_val.get(attr).unwrap();
                self.parse_json_value(cur_lookup + 1, results_vec, new_value, lookup_search);
            }
            (LookupBlock::Pos(ind), Value::Array(sub_values)) => {
                let new_value = sub_values.get(*ind).unwrap();
                self.parse_json_value(cur_lookup + 1, results_vec, new_value, lookup_search);
            }
            (LookupBlock::Att(_), Value::Array(sub_values)) => {
                sub_values.iter().for_each(|sub_value| {
                    self.parse_json_value(cur_lookup, results_vec, sub_value, lookup_search);
                })
            }
            (
                LookupBlock::Take,
                v @ Value::Bool(_) | v @ Value::Number(_) | v @ Value::String(_),
            ) => {
                results_vec.push(v.as_str().unwrap().to_string());
            }
            (lookup, val) => {
                panic!("WRONG VALUE TYPE {} for Loookup {:?}", val, lookup);
            }
        }
    }

    pub fn next_steps(&self) -> &Vec<NextProcessingStep> {
        match self {
            ProcessingStep::Html { next_steps, .. } => next_steps,
            ProcessingStep::Regex { next_steps, .. } => next_steps,
            ProcessingStep::JSON { next_steps, .. } => next_steps,
        }
    }
}
