use hyper_tungstenite::tungstenite::Message;
use serde::{Deserialize, Serialize};

use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
/// Deserializable packet from client
pub enum ClientPacket {
    Message { text: String },
    JoinLobby { id: Uuid },
    CreateLobby,
    ListLobbies,
    Close { info: Option<String> },
}

impl ClientPacket {
    pub fn parse(msg: Message) -> serde_json::Result<ClientPacket> {
        match msg {
            Message::Text(text) => serde_json::from_str(&text),
            Message::Binary(_) => todo!(),
            Message::Ping(_) => todo!(),
            Message::Pong(_) => todo!(),
            Message::Close(_) => todo!(),
            Message::Frame(_) => todo!(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
/// Serializable packet to client
pub enum LobbyPacket {
    LobbyUpdate { current: Uuid },
    Message { text: String },
    Error { err: String },
}

impl LobbyPacket {
    pub fn to_json(self) -> String {
        match serde_json::to_string(&self) {
            Ok(s) => s,
            Err(_) => serde_json::to_string(&LobbyPacket::Error {
                err: String::from("Client side serialization failed"),
            })
            .unwrap(), // `LobbyPacket::Error` will always be serializable
        }
    }
}

// Only LobbyPacket -> Message is allowed
#[allow(clippy::from_over_into)]

impl Into<Message> for LobbyPacket {
    fn into(self) -> Message {
        Message::Text(self.to_json())
    }
}

#[derive(Debug)]
pub enum LobbyRequest {
    Create { lobby_id: Uuid },
    List,
    Change { lobby_id: Uuid },
    None,
}
