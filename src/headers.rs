
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{de, Deserialize, Deserializer};
use serde_json::{Map, Value};
use std::iter::FromIterator;
use std::str::FromStr;

type HeaderDeserializeResult = Result<(HeaderName, HeaderValue), String>;

pub fn de_headers<'de, D>(deserializer: D) -> Result<HeaderMap, D::Error>
where
    D: Deserializer<'de>,
{
    let raw_headers: Map<String, Value> = Deserialize::deserialize(deserializer)?;
    let (headers, errors): (Vec<HeaderDeserializeResult>,
     Vec<HeaderDeserializeResult>) = raw_headers.into_iter().map(
        |(h, val)| 
        {
            match HeaderName::from_str(&h) {
                Ok(header) => {
                    match val {
                        v @ Value::Bool(_) | v @ Value::Number(_) | v @ Value::String(_) => {
                            let value = HeaderValue::from_str(v.as_str().unwrap());
                            match value {
                                Ok(h_value) => Ok((header, h_value)),
                                Err(invalid_value) => Err(invalid_value.to_string())
                            }
                        },
                        invalid_json_value => Err(format!("Invalid header value from JSON {:?}", invalid_json_value))
                        
                    }
                },
                Err(invalid_header) => Err(invalid_header.to_string()) 
            }
        }
    ).partition(|possible_header| {
        possible_header.is_ok()
    });

    if !errors.is_empty() {
        let all_errors = errors.into_iter().map(|err| err.unwrap_err()).collect::<Vec<String>>().join("\n");
        Err(de::Error::custom(all_errors))
    } else {
        Ok(HeaderMap::from_iter(headers.into_iter().map(|header| header.unwrap())))
    }
}