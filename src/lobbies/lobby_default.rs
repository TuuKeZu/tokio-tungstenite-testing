use crate::connection::Connection;
use crate::lobby::*;
use crate::packets::*;
use async_trait::async_trait;
use futures::SinkExt;
use hyper_tungstenite::tungstenite::Message;
use std::any::Any;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Default)]
pub struct LobbyDefault {
    lobby_type: LobbyType,
    id: Uuid,
    pub connections: RwLock<Vec<Arc<Connection>>>,
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
        println!("Lobby [{}] initialized", self.id);
    }

    async fn connection_count(&self) -> usize {
        self.connections.write().await.len()
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

    async fn get_connection(&self, id: &Uuid) -> Option<Arc<Connection>> {
        self.connections
            .read()
            .await
            .iter()
            .find(|conn| id == &conn.id)
            .map(Arc::clone)
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
                    println!("[Lobby {}] Connection {}: {}", self.id, conn_id, text);

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

    async fn join(&self, conn: Arc<Connection>) {
        println!("[Lobby {}] Connection {} has connected", self.id, conn.id);

        self.connections.write().await.push(conn);
    }

    async fn leave(&self, id: &Uuid) {
        println!("[Lobby {}] Connection {} has disconnected", self.id, id);
        self.connections.write().await.retain(|conn| &conn.id != id);
    }
}
