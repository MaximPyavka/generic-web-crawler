use bytes::Bytes;
use encoding_rs::UTF_8;
use reqwest::{header, Response, Result as ReqwestResult};
use serde::{self, Deserialize};
use std::borrow::Cow;

use mime::Mime;
use std::fmt::Debug;

#[derive(Debug, Deserialize, Clone, PartialOrd, Ord, Eq, PartialEq)]
pub enum RespAdaptMarker {
    Text,
    Bytes,
}

impl RespAdaptMarker {
    pub fn is_bytes(&self) -> bool {
        matches!(self, RespAdaptMarker::Bytes)
    }
}

#[derive(Debug, Clone)]
pub enum Resp {
    // Todo Consider changing implementation using Cow
    RespText(String),
    RespBytes {
        bts: Bytes,
        filename: String,
        mime_type: Option<Mime>,
    },
}

impl Resp {
    pub async fn adopt(marker: &RespAdaptMarker, resp: Response) -> ReqwestResult<Resp> {
        match &*marker {
            RespAdaptMarker::Text => Ok(Resp::RespText(resp.text().await?)),
            RespAdaptMarker::Bytes => {
                let mime_type = resp
                    .headers()
                    .get(header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<Mime>().ok());

                let file_type = mime_type.as_ref().map(|mm| mm.subtype().to_string());

                let mut filename = resp
                    .url()
                    .clone()
                    .path_segments()
                    .unwrap_or_else(|| panic!("Failed to get filepath from URL: {:?}", resp.url()))
                    .collect::<Vec<_>>()
                    .join("_");

                if file_type.is_some() && !filename.ends_with(file_type.as_ref().unwrap()) {
                    filename.push_str(&format!(".{}", file_type.unwrap()));
                }

                // println!("NEW FILENAME {}", filename);

                Ok(Resp::RespBytes {
                    bts: resp.bytes().await?,
                    filename,
                    mime_type,
                })
            }
        }
    }

    pub fn res_type_marker(&self) -> RespAdaptMarker {
        match self {
            Resp::RespText(_) => RespAdaptMarker::Text,
            Resp::RespBytes { .. } => RespAdaptMarker::Bytes,
        }
    }

    pub fn _bytes_to_text(&self) -> Self {
        // TODO retrieve content-type charset from resp headers
        let encoding = UTF_8;

        let bts = match self {
            Resp::RespBytes { bts, .. } => bts,
            _ => panic!(),
        };

        let (text, _, _) = encoding.decode(bts);
        if let Cow::Owned(s) = text {
            return Resp::RespText(s);
        }
        unsafe {
            // decoding returned Cow::Borrowed, meaning these bytes
            // are already valid utf8
            Resp::RespText(String::from_utf8_unchecked(bts.clone().to_vec()))
        }
    }

    pub fn from_bytes(&self, convert_to: &RespAdaptMarker) -> Self {
        if self.res_type_marker() != RespAdaptMarker::Bytes {
            println!(
                "Trying convert response from Bytes, but self type is {:?}",
                self.res_type_marker()
            );
            panic!();
        } else if convert_to == &RespAdaptMarker::Bytes {
            println!(
                "Trying to convert from Bytes to {:?}, which doesn't make sense",
                convert_to
            );
            panic!();
        } else {
            // Now only text type is supported
            self._bytes_to_text()
        }
    }
}
