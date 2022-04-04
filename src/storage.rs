use bytes::Bytes;
use nanoid::nanoid;
use serde::{de, Deserialize, Deserializer};
use serde_json::{Map, Value};
use std::io::Cursor;
use std::path::{Path, PathBuf};

use google_drive3::{api, hyper, hyper_rustls, DriveHub, Error};
use mime::{self, Mime, STAR_STAR};
use std::default::Default;
use std::fs::create_dir_all;
use tokio::fs::write;

use yup_oauth2 as oauth;

use crate::response_adaptor::Resp;

#[derive(std::fmt::Debug, Deserialize, Clone)]
pub enum FileName {
    RandomNanoid,
    Origin,
}

impl Default for FileName {
    fn default() -> Self {
        FileName::Origin
    }
}

#[derive(std::fmt::Debug, Deserialize, Clone)]
pub enum FileExt {
    MP4,
    JPEG,
}

fn try_local_path(path: &str, or_create: bool) -> Result<PathBuf, String> {
    let path_buf = PathBuf::from(path);
    if path_buf.exists() {
        Ok(path_buf)
    } else {
        match or_create {
            true => match create_dir_all(&path_buf) {
                Ok(_) => Ok(path_buf),
                Err(e) => Err(e.to_string()),
            },
            false => Err(format!("Directory {:?} doesn't exists...", path)),
        }
    }
}

fn de_binary<'de, D>(deserializer: D) -> Result<PathBuf, D::Error>
where
    D: Deserializer<'de>,
{
    let path_map: Map<String, Value> = Deserialize::deserialize(deserializer)?;

    let path_opt = path_map.get("path");
    let or_create = path_map.get("or_create");
    match path_opt {
        Some(path) => {
            if !path.is_string() {
                return Err(de::Error::custom(format!(
                    "Wrong value for 'path', {:?}",
                    path.to_string()
                )));
            }

            let create = match or_create.or(Some(&Value::Bool(false))).unwrap() {
                Value::Bool(b) => *b,
                wrong_bool => {
                    return Err(de::Error::custom(format!(
                        "Wrong value for 'or_create', {:?}",
                        wrong_bool.to_string()
                    )))
                }
            };

            match try_local_path(path.as_str().unwrap(), create) {
                Err(e) => Err(de::Error::custom(e)),
                Ok(path) => Ok(path),
            }
        }
        None => Err(de::Error::custom(format!(
            "Path variable has not been passed for directory, {:?}",
            path_map
        ))),
    }
}

#[derive(Debug, Deserialize, Clone)]
pub enum Storage {
    LocalDrive {
        #[serde(deserialize_with = "de_binary")]
        dirname: PathBuf,
        #[serde(default)]
        filename_class: FileName,
        #[serde(skip_serializing_if = "Option::is_none")]
        ext: Option<FileExt>,
    },
    GoogleDrive {
        folder_id: String,
    },
}

impl Storage {
    pub async fn store(&self, resp: &Resp) {
        let (bytes_result, filename, mime_type) = match resp {
            Resp::RespBytes {
                bts,
                filename,
                mime_type,
            } => (bts, filename, mime_type),
            _ => {
                unreachable!("For now")
            }
        };

        match self {
            Storage::LocalDrive {
                dirname,
                filename_class,
                ext,
            } => {
                self.store_local(
                    bytes_result,
                    filename,
                    dirname,
                    filename_class,
                    ext.as_ref(),
                )
                .await;
            }
            Storage::GoogleDrive { folder_id } => {
                self.store_in_google_drive(
                    bytes_result,
                    filename,
                    folder_id,
                    mime_type.as_ref().unwrap_or(&STAR_STAR),
                )
                .await;
            }
        }
    }

    pub async fn store_local(
        &self,
        bytes_result: &Bytes,
        filename: &str,
        dest_dir: &Path,
        filename_class: &FileName,
        ext: Option<&FileExt>,
    ) {
        let mut content_name = dest_dir.to_owned();
        let updated_filename = self.prepare_filename(filename, filename_class, ext);
        content_name.push(updated_filename);
        if !Path::new(&content_name).exists() {
            if let Err(e) = write(&content_name, bytes_result).await {
                eprintln!("Failed to create file with content {:?}", e)
            }
            else {
                println!("Created new content in {:?}", content_name)
            }
        } else {
            println!("Skip {:?}", content_name);
        };
    }

    pub async fn store_in_google_drive(
        &self,
        bytes_result: &Bytes,
        filename: &str,
        google_forlder_id: &str,
        mime_type: &Mime,
    ) {
        let key = oauth::read_service_account_key(
            "./json_templates/sa.json",
        )
        .await
        .unwrap();

        let auth = yup_oauth2::ServiceAccountAuthenticator::builder(key)
            .persist_tokens_to_disk("tokencache.json")
            .build()
            .await
            .expect("failed to create authenticator");

        let hub = DriveHub::new(
            hyper::Client::builder().build(hyper_rustls::HttpsConnector::with_native_roots()),
            auth,
        );
        let file_to_upload = api::File {
            parents: Some(vec![google_forlder_id.to_string()]),
             name: Some(filename.to_string()),
            ..Default::default()};

        let bytes_cursor = Cursor::new(bytes_result);

        let result = hub
            .files()
            .create(file_to_upload)
            .upload_resumable(bytes_cursor, mime_type.clone().to_string().parse().unwrap())
            .await;

        match result {
            Err(e) => match e {
                Error::HttpError(_)
                | Error::Io(_)
                | Error::MissingAPIKey
                | Error::MissingToken(_)
                | Error::Cancelled
                | Error::UploadSizeLimitExceeded(_, _)
                | Error::Failure(_)
                | Error::BadRequest(_)
                | Error::FieldClash(_)
                | Error::JsonDecodeError(_, _) => println!("{}", e),
            },
            Ok(res) => {
                println!("Status of upload to GD: {:?}", res.0.status())
            }
        }
    }

    pub fn prepare_filename(
        &self,
        filename: &str,
        filename_class: &FileName,
        ext: Option<&FileExt>,
    ) -> String {
        let mut new_filename = match filename_class {
            FileName::Origin => filename.to_owned(),
            FileName::RandomNanoid => {
                nanoid!(10)
            }
        };
        if let Some(file_ext) = ext {
            {
                let file_ext_str = match file_ext {
                    FileExt::JPEG => ".jpeg",
                    FileExt::MP4 => ".mp4",
                };
                new_filename.push_str(file_ext_str);
            }
        } else {}
        new_filename
    }
}
