use futures::Future;
use std::pin::Pin;

use tokio::sync::mpsc::Sender;

pub type PinnedFutureSender = Sender<Pin<Box<(dyn Future<Output = ()> + Send)>>>;