use crate::connection::Connection;
use crate::lobby::*;
use async_trait::async_trait;
use futures::SinkExt;
use hyper_tungstenite::tungstenite::Message;
use std::any::Any;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use colored::*;
use serde::{Deserialize, Serialize};

type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Default)]
pub struct LobbyDefault {
    lobby_type: LobbyType,
    id: Uuid,
    connections: RwLock<Vec<Arc<Connection>>>,
}

#[async_trait]
impl Lobby for LobbyDefault {
    fn new(id: Uuid) -> LobbyDefault {
        LobbyDefault {
            lobby_type: LobbyType::Default,
            id,
            connections: RwLock::new(vec![]),
        }
    }

    fn default() -> LobbyDefault {
        LobbyDefault {
            lobby_type: LobbyType::Default,
            id: Uuid::new_v4(),
            connections: RwLock::new(vec![]),
        }
    }

    fn get_id(&self) -> Uuid {
        self.id
    }

    fn get_type(&self) -> LobbyType {
        self.lobby_type
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    fn initialize(&self) {
        println!("{} {}", self, format!("Initialized").green().bold());
    }

    async fn connection_count(&self) -> usize {
        self.connections.write().await.len()
    }

    
    async fn broadcast(&self, msg: Message) -> Result<(), Error> {
        for connection in self.connections.read().await.iter() {
            connection
                .sink
                .lock()
                .await
                .send(msg.clone().try_into()?)
                .await?;
        }
        
        Ok(())
    }

    async fn get_connection(&self, id: &Uuid) -> Option<Arc<Connection>> {
        self.connections
            .read()
            .await
            .iter()
            .find(|conn| id == &conn.id)
            .map(Arc::clone)
    }

    async fn emit(&self, conn_id: &Uuid, msg: Message) -> Result<(), Error> {
        
        if let Some(conn) = self.get_connection(conn_id).await {
            conn.sink.lock().await.send(msg.clone().try_into()?).await?;
        } else {
            eprintln!("Cannot emit to non-existent connection {conn_id}");
        }
        
        Ok(())
    }
    

    async fn handle_message(&self, msg: Message, conn_id: Uuid) -> Result<LobbyRequest, Error> {
        match msg.try_into()? {
            ClientPacket::Message { text } => {
                println!("{} Connection {}: {}", self, conn_id, text);

                self.broadcast(ServerPacket::Message { text }.try_into()?).await?;
            }
            ClientPacket::JoinLobby { id } => {
                return Ok(LobbyRequest::Change { lobby_id: id });
            }
            ClientPacket::CreateLobby => {
                return Ok(LobbyRequest::Create {
                    lobby_id: Uuid::new_v4(),
                })
            }
            ClientPacket::ListLobbies => {
                return Ok(LobbyRequest::List);
            }
            ClientPacket::Close { info: _ } => {}
            ClientPacket::Error { err } => {
                self.emit(&conn_id, ServerPacket::Error { err }.try_into()?).await?;
            }
        }
        
        
        Ok(LobbyRequest::None)
    }

    async fn join(&self, conn: Arc<Connection>) -> Result<(), Error> {
        let conn_id = conn.id;
        self.connections.write().await.push(conn);
        
        self.emit(&conn_id, ServerPacket::LobbyUpdate { current: self.id }.try_into()?).await?;
        
        println!("{} Connection {} has connected", self, conn_id);
        Ok(())
    }

    async fn leave(&self, id: &Uuid) -> Result<(), Error> {
        println!("{} Connection {} has disconnected", self, id);
        self.connections.write().await.retain(|conn| &conn.id != id);
        Ok(())
    }
}

impl std::fmt::Display for LobbyDefault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format!("[Lobby {}]", self.get_id()).dimmed())
    }
}

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

    fn try_from(msg: Message) -> Result<Self, <Self as TryFrom<Message>>::Error> {
        match msg {
            Message::Text(text) => {
                serde_json::from_str(&text).map_err(<Self as TryFrom<Message>>::Error::from)
            },
            Message::Binary(_) => panic!("yeet"),
            Message::Ping(_) => panic!("yeet"),
            Message::Pong(_) => panic!("yeet"),
            Message::Close(_) => panic!("yeet"),
            Message::Frame(_) => panic!("yeet"),
        }
    }
}

impl TryInto<Message> for ClientPacket {
    type Error = Error;
    fn try_into(self) -> Result<Message, <Self as TryFrom<Message>>::Error> {
        Ok(Message::Text(serde_json::to_string(&self)?))
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
        match msg {
            Message::Text(text) => {
                serde_json::from_str(&text).map_err(<Self as TryFrom<Message>>::Error::from)
            },
            Message::Binary(_) => panic!("yeet"),
            Message::Ping(_) => panic!("yeet"),
            Message::Pong(_) => panic!("yeet"),
            Message::Close(_) => panic!("yeet"),
            Message::Frame(_) => panic!("yeet"),
        }
    }
}

impl TryInto<Message> for ServerPacket {
    type Error = Error;

    fn try_into(self) -> Result<Message, <Self as TryFrom<Message>>::Error> {
        Ok(Message::Text(serde_json::to_string(&self)?))
    }
}
