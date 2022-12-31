use crate::connection::Connection;
use crate::lobby::*;
use async_trait::async_trait;
use futures::SinkExt;
use futures::stream::SplitSink;
use hyper_tungstenite::WebSocketStream;
use hyper_tungstenite::tungstenite::Message;
use std::{any::Any, collections::HashMap};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use colored::*;
use hyper::upgrade::Upgraded;
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};

type Error = Box<dyn std::error::Error + Send + Sync>;
type WebsocketSink = Arc<Mutex<SplitSink<WebSocketStream<Upgraded>, Message>>>;

#[derive(Debug)]
struct ChatUser {
    username: String,
}

impl ChatUser {
    fn new(username: String) -> ChatUser {
        ChatUser {
            username
        }
    }
}

#[derive(Debug, Default)]
pub struct LobbyChat {
    lobby_type: LobbyType,
    id: Uuid,
    connections: RwLock<Vec<Arc<Connection>>>,
    mapping: RwLock<HashMap<Uuid, ChatUser>>,
}

impl LobbyChat {
    
}

trait Scheme: Serialize {
    fn version(&self) -> (u32, u32, u32);
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetUsernameScheme {
    version: (u32, u32, u32),
}

impl Scheme for SetUsernameScheme {
    fn version(&self) -> (u32, u32, u32) {
        self.version
    }
}

const POGCHAT_SET_USERNAME_SCHEME: SetUsernameScheme = SetUsernameScheme {
    version: (1, 0, 0)
};

impl LobbyChat {

}

#[async_trait]
impl Lobby for LobbyChat {
    fn new(id: Uuid) -> Self {
        Self {
            lobby_type: LobbyType::Default,
            id,
            connections: RwLock::new(vec![]),
            mapping: RwLock::new(HashMap::new()),
        }
    }

    fn default() -> Self {
        Self {
            lobby_type: LobbyType::Default,
            id: Uuid::new_v4(),
            connections: RwLock::new(vec![]),
            mapping: RwLock::new(HashMap::new()),
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
        self.connections.read().await.len()
    }

    
    async fn broadcast(&self, msg: Message) -> Result<(), Error> {
        for connection in self.connections.read().await.iter() {
            if self.mapping.read().await.contains_key(&connection.id) {
             
            connection
                .sink
                .lock()
                .await
                .send(msg.clone().try_into()?)
                .await?;   
            }
        }
        
        Ok(())
    }

    async fn get_connection(&self, id: &Uuid) -> Option<Arc<Connection>> {
        self.connections
            .read()
            .await
            .iter()
            .find(|conn| &conn.id == id)
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
            },
            ClientPacket::SetUsername { username } => {
                self.mapping.write().await.entry(conn_id).or_insert(ChatUser::new(username));
                self.broadcast(ServerPacket::RoomUpdate { users: self.mapping.read().await.values().map(|ChatUser { ref username }| username).cloned().collect() }.try_into()?).await?;
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
        self.emit(&conn_id, ServerPacket::RequestUsername { scheme: POGCHAT_SET_USERNAME_SCHEME }.try_into()?).await?;
        
        println!("{} Connection {} has connected", self, conn_id);
        Ok(())
    }

    async fn leave(&self, id: &Uuid) -> Result<(), Error> {
        self.broadcast(ServerPacket::RoomUpdate { users: self.mapping.read().await.values().map(|ChatUser { ref username }| username).cloned().collect() }.try_into()?).await?;
        
        self.connections.write().await.retain(|conn| &conn.id != id);
        println!("{} Connection {} has disconnected", self, id);
        Ok(())
    }
}

impl std::fmt::Display for LobbyChat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format!("[Chat {}]", self.get_id()).dimmed())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientPacket {
    Message { text: String },
    JoinLobby { id: Uuid },
    CreateLobby,
    ListLobbies,
    SetUsername { username: String },
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
    RequestUsername { scheme: SetUsernameScheme },
    RoomUpdate { users: Vec<String> },
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
