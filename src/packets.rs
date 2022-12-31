///////////////////////////////////////////////////////////
///////////////////////////////////////////////////////////
////////////////////// DELET THIS /////////////////////////
///////////////////////////////////////////////////////////
///////////////////////////////////////////////////////////


use std::any::Any;
use enum_dispatch::enum_dispatch;

use hyper_tungstenite::tungstenite::Message;
use serde::{Deserialize, Serialize};

use uuid::Uuid;

type Error = Box<dyn std::error::Error + Send + Sync>;


/// General trait for all packets
pub trait LobbyPacket: Send + Sync + Sized + TryFrom<Message> + TryInto<Message> {}

/// Deserializable packet from client
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientPacket {
    Message { text: String },
    JoinLobby { id: Uuid },
    CreateLobby,
    ListLobbies,
    Close { info: Option<String> },
    Error { err: String }
}

impl TryFrom<Message> for ClientPacket {
    type Error = Error;

    fn try_from(value: Message) -> Result<Self, <Self as TryFrom<Message>>::Error> {
        todo!()
    }
}

impl TryInto<Message> for ClientPacket {
    type Error = Error;
    fn try_into(self) -> Result<Message, <Self as TryFrom<Message>>::Error> {
        todo!()
    }
}


/// Serializable packet to client
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerPacket {
    LobbyUpdate { current: Uuid },
    Message { text: String },
    Error { err: String },
}

impl TryFrom<Message> for ServerPacket {
    type Error = Error;

    fn try_from(msg: Message) -> Result<Self, <Self as TryFrom<Message>>::Error> {
        todo!()
    }
}

impl TryInto<Message> for ServerPacket {
    type Error = Error;

    fn try_into(self) -> Result<Message, <Self as TryInto<Message>>::Error> {
        todo!()
    }
}


#[derive(Debug)]
pub enum LobbyRequest {
    Create { lobby_id: Uuid },
    List,
    Change { lobby_id: Uuid },
    None,
}
