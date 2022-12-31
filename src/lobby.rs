use crate::connection::Connection;

use async_trait::async_trait;
use hyper_tungstenite::tungstenite::Message;
use std::any::Any;
use std::fmt::Display;
use std::sync::Arc;
use uuid::Uuid;
use colored::*;


type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LobbyType {
    Default,
}

impl Default for LobbyType {
    fn default() -> Self {
        LobbyType::Default
    }
}

#[async_trait]
pub trait Lobby: Send + Sync + Display {
    /// Creates new lobby struct with specified uuid
    fn new(id: Uuid) -> Self
    where
        Self: Sized;

    /// Creates new lobby struct with randomly generated uuid
    fn default() -> Self
    where
        Self: Sized;

    /// Returns id of the lobby
    fn get_id(&self) -> Uuid;

    /// Returns type of the lobby
    fn get_type(&self) -> LobbyType;

    /// Returns lobby as `dyn Any`
    fn as_any(&mut self) -> &mut dyn Any;

    /// Run rtight at the creation of the lobby. Custom processes and functionalities should be initialized here
    fn initialize(&self);

    /// Returns the number of connections in lobby
    async fn connection_count(&self) -> usize;

    /// Broacasts a packet to all connected clients
    async fn broadcast(&self, msg: Message) -> Result<(), Error>;

    /// Returns a `Option<Arc<Connection>>` from specified uuid
    async fn get_connection(&self, conn_id: &Uuid) -> Option<Arc<Connection>>;

    /// Emits a packet to specific connection
    /// TODO move emit to [`Connection`]
    async fn emit(&self, conn_id: &Uuid, msg: Message) -> Result<(), Error>;

    /// Handles events. Lobby's logic should happen here
    async fn handle_message(&self, msg: Message, id: Uuid) -> Result<LobbyRequest, Error>;

    /// Add client to the lobby
    async fn join(&self, conn: Arc<Connection>) -> Result<(), Error>;

    /// removes client from the lobby
    async fn leave(&self, id: &Uuid) -> Result<(), Error>;

    /// Returns `true` if the lobby's type is `LobbyType::Default`
    fn is_default(&self) -> bool {
        self.get_type() == LobbyType::Default
    }

    /// returns `true` if the lobby doesn't have any connections
    async fn is_empty(&self) -> bool {
        self.connection_count().await == 0
    }
}

#[derive(Debug)]
pub enum LobbyRequest {
    Create { lobby_id: Uuid },
    List,
    Change { lobby_id: Uuid },
    None,
}

