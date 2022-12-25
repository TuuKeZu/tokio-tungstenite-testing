use futures::stream::SplitSink;
use hyper::upgrade::Upgraded;
use hyper_tungstenite::tungstenite::Message;
use hyper_tungstenite::WebSocketStream;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

type Error = Box<dyn std::error::Error + Send + Sync>;
type WebSocketSink = Arc<Mutex<SplitSink<WebSocketStream<Upgraded>, Message>>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
/// Deserializable packet from client
pub enum ClientPacket {
    Message { text: String },
    Close { info: Option<String> },
}

impl ClientPacket {
    fn new(msg: Message) -> Result<ClientPacket, Error> {
        match msg {
            Message::Text(text) => Ok(ClientPacket::Message { text }),
            Message::Binary(_) => todo!(),
            Message::Ping(_) => todo!(),
            Message::Pong(_) => todo!(),
            Message::Close(info) => Ok(ClientPacket::Close {
                info: info.map(|f| f.to_string()),
            }),
            Message::Frame(_) => todo!(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
/// Serializable packet to client
pub enum LobbyPacket {
    Message { text: String },
}

impl Into<Message> for LobbyPacket {
    fn into(self) -> Message {
        match self {
            LobbyPacket::Message { text } => Message::Text(text),
        }
    }
}

#[derive(Debug)]
pub enum LobbyRequest {
    Create { lobby_id: Uuid },
    List,
    Change { lobby_id: Uuid },
    None,
}
