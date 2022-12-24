use futures::stream::SplitSink;
use futures::SinkExt;
use hyper::upgrade::Upgraded;
use hyper_tungstenite::tungstenite::Message;
use hyper_tungstenite::WebSocketStream;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::{collections::HashMap, fmt};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

type Error = Box<dyn std::error::Error + Send + Sync>;
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

/// Default struct for lobbies. Includes the base functionalities for lobby.
pub trait DefaultLobby {
    fn new(id: Uuid) -> Self;

    fn default() -> Self;

    async fn get_connection(&self, id: &Uuid) -> Option<Arc<Connection>>;

    async fn broadcast(&self, packet: LobbyPacket) -> Result<(), Error>;

    async fn emit(&self, conn_id: &Uuid, msg: LobbyPacket) -> Result<(), Error>;

    async fn handle_message(&self, msg: Message, id: Uuid) -> Result<LobbyRequest, Error>;

    async fn join(&self, conn: Arc<Connection>);

    async fn leave(&self, id: &Uuid);
}

#[derive(Default)]
pub struct Lobby {
    pub id: Uuid,
    pub connections: RwLock<Vec<Arc<Connection>>>,
}

impl DefaultLobby for Lobby {
    fn new(id: Uuid) -> Lobby {
        Lobby {
            id,
            connections: RwLock::new(vec![]),
        }
    }

    fn default() -> Lobby {
        Lobby {
            id: Uuid::new_v4(),
            connections: RwLock::new(vec![]),
        }
    }

    async fn get_connection(&self, id: &Uuid) -> Option<Arc<Connection>> {
        self.connections
            .read()
            .await
            .iter()
            .find(|conn| id == &conn.id)
            .map(Arc::clone)
    }

    async fn broadcast(&self, packet: LobbyPacket) -> Result<(), Error> {
        for connection in self.connections.read().await.iter() {
            connection
                .sink
                .lock()
                .await
                .send(packet.clone().into())
                .await?;
        }

        Ok(())
    }

    async fn emit(&self, conn_id: &Uuid, msg: LobbyPacket) -> Result<(), Error> {
        if let Some(conn) = self.get_connection(conn_id).await {
            conn.sink.lock().await.send(msg.clone().into()).await?;
        } else {
            eprintln!("Cannot emit to non-existent connection {conn_id}");
        }

        Ok(())
    }

    async fn handle_message(&self, msg: Message, id: Uuid) -> Result<LobbyRequest, Error> {
        match msg.clone() {
            Message::Text(text) => {
                match text.as_str() {
                    "list" => return Ok(LobbyRequest::List),
                    "create" => {
                        return Ok(LobbyRequest::Create {
                            lobby_id: Uuid::new_v4(),
                        })
                    }
                    "change" => return Ok(LobbyRequest::Change { lobby_id: self.id }),
                    &_ => {}
                }

                println!("[Lobby {}] Connection {}: {}", self.id, id, text);
                self.broadcast(LobbyPacket::Message { text: text.clone() })
                    .await?
            }
            Message::Binary(_) => todo!(),
            Message::Ping(_) => todo!(),
            Message::Pong(_) => todo!(),
            Message::Close(_) => unreachable!(),
            Message::Frame(_) => todo!(),
        }

        Ok(LobbyRequest::None)
    }

    async fn join(&self, conn: Arc<Connection>) {
        println!("[Lobby {}] Connection {} has connected", self.id, conn.id);
        // handle connection joining
        self.connections.write().await.push(conn);
    }

    async fn leave(&self, id: &Uuid) {
        println!("[Lobby {}] Connection {} has disconnected", self.id, id);
        // handle connection leaving
        self.connections.write().await.retain(|conn| &conn.id != id);
    }
}

impl fmt::Debug for Lobby {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Lobby [{}]", self.id)
    }
}

impl fmt::Display for Lobby {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Lobby [{}]", self.id)
    }
}
