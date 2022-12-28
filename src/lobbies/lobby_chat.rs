use crate::connection::Connection;
use crate::lobby::*;
use crate::packets::*;
use async_trait::async_trait;
use futures::SinkExt;
use futures::stream::SplitSink;
use hyper_tungstenite::WebSocketStream;
use hyper_tungstenite::tungstenite::Message;
use std::any::Any;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use colored::*;
use hyper::upgrade::Upgraded;
use tokio::sync::Mutex;

type Error = Box<dyn std::error::Error + Send + Sync>;
type WebsocketSink = Arc<Mutex<SplitSink<WebSocketStream<Upgraded>, Message>>>;

#[derive(Debug)]
struct ChatUser {
    username: RwLock<Option<String>>,
    connection: Arc<Connection>,
    initialized: bool,
}

impl ChatUser {
    fn new(connection: Arc<Connection>) -> ChatUser {
        ChatUser {
            username: RwLock::new(None),
            connection,
            initialized: false
        }
    }

    fn get_id(&self) -> Uuid {
        self.connection.id
    }

    fn get_sink(&self) -> &WebsocketSink {
        &self.connection.sink
    }

    async fn get_username(&self) -> Option<String> {
        self.username.read().await.to_owned()
    }

    fn is_initialized(&self) -> bool {
        self.initialized
    }

    fn get_connection(&self) -> Arc<Connection> {
        Arc::clone(&self.connection)
    }
}

#[derive(Debug, Default)]
pub struct LobbyChat {
    lobby_type: LobbyType,
    id: Uuid,
    users: RwLock<Vec<Arc<ChatUser>>>,
    chat: RwLock<Vec<String>>
}

impl LobbyChat {
    async fn get_user(&self, conn_id: Uuid) -> Option<Arc<ChatUser>> {
        self.users.read().await.iter().find(|user| user.get_id() == conn_id).map(Arc::clone)
    }

    async fn register(&self, conn_id: Uuid, username: String) {
        if let Some(user) = self.get_user(conn_id).await {
            if !user.is_initialized() {
                *user.username.write().await = Some(username);
            }
        }
    }
}

#[async_trait]
impl Lobby for LobbyChat {
    fn new(id: Uuid) -> LobbyChat {
        LobbyChat {
            lobby_type: LobbyType::Default,
            id,
            users: RwLock::new(vec![]),
            chat: RwLock::new(vec![])
        }
    }

    fn default() -> LobbyChat {
        LobbyChat {
            lobby_type: LobbyType::Default,
            id: Uuid::new_v4(),
            users: RwLock::new(vec![]),
            chat: RwLock::new(vec![])
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
        self.users.write().await.len()
    }

    async fn broadcast(&self, packet: LobbyPacket) -> Result<(), Error> {
        for user in self.users.read().await.iter().filter(|user| user.is_initialized()) {
            user
                .get_sink()
                .lock()
                .await
                .send(packet.clone().into())
                .await?;
        }

        Ok(())
    }

    async fn get_connection(&self, id: &Uuid) -> Option<Arc<Connection>> {
        self.users
            .read()
            .await
            .iter()
            .find(|user| id == &user.get_id())
            .map(|c| c.get_connection())
    }

    async fn emit(&self, conn_id: &Uuid, msg: LobbyPacket) -> Result<(), Error> {
        if let Some(conn) = self.get_connection(conn_id).await {
            conn.sink.lock().await.send(msg.clone().into()).await?;
        } else {
            eprintln!("Cannot emit to non-existent connection {conn_id}");
        }

        Ok(())
    }

    async fn handle_message(&self, msg: Message, conn_id: Uuid) -> Result<LobbyRequest, Error> {
        let json = ClientPacket::parse(msg);

        match json {
            Ok(packet) => match packet {
                ClientPacket::Message { text } => {
                    println!("{} Connection {}: {}", self, conn_id, text);

                    self.broadcast(LobbyPacket::Message { text }).await?;
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
            },
            Err(e) => {
                self.emit(&conn_id, LobbyPacket::Error { err: e.to_string() })
                    .await?;
            }
        }

        Ok(LobbyRequest::None)
    }

    async fn join(&self, conn: Arc<Connection>) -> Result<(), Error> {
        let conn_id = conn.id;
        let user = ChatUser::new(conn);
        self.users.write().await.push(Arc::new(user));
        
        self.emit(&conn_id, LobbyPacket::LobbyUpdate { current: self.id }).await?;
        println!("{} Connection {} has connected", self, conn_id);
        Ok(())
    }

    async fn leave(&self, id: &Uuid) -> Result<(), Error> {
        println!("{} Connection {} has disconnected", self, id);
        self.users.write().await.retain(|user| &user.get_id() != id);
        Ok(())
    }
}

impl std::fmt::Display for LobbyChat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format!("[Chat {}]", self.get_id()).dimmed())
    }
}
