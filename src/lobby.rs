use crate::connection::Connection;
use crate::packets::{LobbyPacket, LobbyRequest};

use async_trait::async_trait;
use hyper_tungstenite::tungstenite::Message;
use std::marker::PhantomData;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

type Error = Box<dyn std::error::Error + Send + Sync>;

#[async_trait]
pub trait Lobby: Send + Sync {
    fn default() -> Self
    where
        Self: Sized;

    fn get_id(&self) -> Uuid;

    async fn broadcast(&self, packet: LobbyPacket) -> Result<(), Error>;

    async fn get_connection(&self, id: &Uuid) -> Option<Arc<Connection>>;

    async fn emit(&self, conn_id: &Uuid, msg: LobbyPacket) -> Result<(), Error>;

    async fn handle_message(&self, msg: Message, id: Uuid) -> Result<LobbyRequest, Error>;

    async fn join(&self, conn: Arc<Connection>);

    async fn leave(&self, id: &Uuid);
}
/*
#[derive(Debug, Default)]
pub struct Lobby<T: LobbyType + ?Sized> {
    pub id: Uuid,
    pub connections: RwLock<Vec<Arc<Connection>>>,
    pub lobby_type: PhantomData<T>,
}

impl Lobby for Lobby<dyn LobbyType> {
    fn new(id: Uuid) -> Lobby<dyn LobbyType> {
        Lobby {
            id,
            connections: RwLock::new(vec![]),
            lobby_type: PhantomData,
        }
    }

    fn default() -> Lobby<dyn LobbyType> {
        Lobby {
            id: Uuid::new_v4(),
            connections: RwLock::new(vec![]),
            lobby_type: PhantomData,
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
        /*
        let a = self.get_connection(&Uuid::new_v4()).await.unwrap();

        for connection in self.connections.read().await.iter() {
            connection
                .sink
                .lock()
                .await
                .send(packet.clone().into())
                .await?;
        }
        */
        Ok(())
    }

    async fn emit(&self, conn_id: &Uuid, msg: LobbyPacket) -> Result<(), Error> {
        /*
        if let Some(conn) = self.get_connection(conn_id).await {
            conn.sink.lock().await.send(msg.clone().into()).await?;
        } else {
            eprintln!("Cannot emit to non-existent connection {conn_id}");
        }
        */
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
*/
