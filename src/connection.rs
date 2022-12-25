use futures::stream::SplitSink;
use hyper::upgrade::Upgraded;
use hyper_tungstenite::tungstenite::Message;
use hyper_tungstenite::WebSocketStream;
use std::fmt;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

type WebSocketSink = Arc<Mutex<SplitSink<WebSocketStream<Upgraded>, Message>>>;

pub struct Connection {
    pub id: Uuid,
    pub sink: WebSocketSink,
}

impl Connection {
    pub fn new(id: Uuid, sink: WebSocketSink) -> Connection {
        Connection { id, sink }
    }
}

impl fmt::Debug for Connection {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Connection [{}]", self.id)
    }
}

impl fmt::Display for Connection {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Connection [{}]", self.id)
    }
}
