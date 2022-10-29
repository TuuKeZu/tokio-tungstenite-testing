use futures::stream::SplitSink;
use futures::SinkExt;
use hyper::upgrade::Upgraded;
use hyper_tungstenite::tungstenite::Message;
use hyper_tungstenite::WebSocketStream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
/// Deserializable packet form client
pub enum ClientPacket {
    Message { text: String },
    Close { info: Option<String> },
}

pub enum ChangeLobbyMessage {
    Change(Uuid),
    None,
}

// trait Lobby {
//     type ClientPacket;

//     fn get_id(&self) -> Uuid;

//     fn handle_message(&mut self, ClientPacket) -> ChangeLobbyMessage;

//     fn join(&mut self, SplitSink<WebSocketStream<Upgraded>, Message>);

//     fn exit(&mut self, usize);
// }

impl ClientPacket {
    fn new(msg: Message) -> Result<ClientPacket, Error> {
        // TODO should return Result
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

pub struct Lobby {
    pub id: Uuid,
    pub connections: RwLock<Vec<Connection>>,
}

impl Lobby {
    pub fn new() -> Lobby {
        Lobby {
            id: Uuid::new_v4(),
            connections: RwLock::new(vec![]),
        }
    }

    async fn get_connection(&self, id: &Uuid) -> Option<WebSocketSink> {
        self.connections
            .read()
            .await
            .iter()
            .find(|conn| id == &conn.id)
            .map(|conn| Arc::clone(&conn.sink))
    }

    async fn broadcast(&self, packet: LobbyPacket) -> Result<(), Error> {
        for connection in self.connections.read().await.iter() {
            println!("broadcasting to {}", connection.id);
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
        if let Some(sink) = self.get_connection(conn_id).await {
            sink.lock().await.send(msg.clone().into()).await?;
        } else {
            eprintln!("Cannot emit to non-existent connection {conn_id}");
        }

        Ok(())
    }

    pub async fn handle_message(
        &self,
        msg: Message,
        id: Uuid,
    ) -> Result<ChangeLobbyMessage, Error> {
        match msg.clone() {
            Message::Text(text) => {
                println!("[Lobby {}] Connection {}: {}", self.id, id, text);
                self.broadcast(LobbyPacket::Message { text: text.clone() })
                    .await?;

                // if something happens:
                /*
                match &text.split(' ').collect::<Vec<_>>()[..] {
                    &["change", id] => {
                        return Ok(ChangeLobbyMessage::Change(
                            id.parse::<Uuid>().unwrap(), /* Fixme comes from client */
                        ));
                    }
                    _ => eprintln!("definitely no other commands than change are allowed"),
                }
                */
            }
            Message::Binary(_) => todo!(),
            Message::Ping(_) => todo!(),
            Message::Pong(_) => todo!(),
            Message::Close(_info) => {
                self.leave(&id).await;
            }
            Message::Frame(_) => todo!(),
        }

        Ok(ChangeLobbyMessage::None)
    }

    pub async fn join(&self, id: Uuid, sink: WebSocketSink) {
        println!("[Lobby {}] Connection {} has connected", self.id, id);
        // handle connection joining
        self.connections
            .write()
            .await
            .push(Connection::new(id, sink));
    }

    pub async fn leave(&self, id: &Uuid) {
        println!("[Lobby {}] Connection {} has disconnected", self.id, id);
        // handle connection leaving
        self.connections.write().await.retain(|conn| &conn.id != id);
    }
}
