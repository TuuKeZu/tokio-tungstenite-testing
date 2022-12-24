#![feature(allocator_api)]
mod lobby;

use futures::stream::{SplitSink, StreamExt};
use futures::SinkExt;
use hyper::server::conn;
use hyper::service::{make_service_fn, service_fn};
use hyper::upgrade::Upgraded;
use hyper::{Body, Request, Response, Server};
use hyper_tungstenite::tungstenite::Message;
use hyper_tungstenite::{HyperWebsocket, WebSocketStream};
use lobby::{Connection, Lobby, LobbyRequest};
use std::alloc::Global;
use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, RwLockWriteGuard};
use uuid::Uuid;

type Error = Box<dyn std::error::Error + Send + Sync>;

/// What should the websocket interface support (the customer asks):
/// 1. Creating a new connection (trivial)
/// 2. a message like `broadcast: msg` the `msg` is sent to all clients in the same lobby
/// 3. a message like `change ID` changes the lobby of the connection to lobby ID
/// 4. a message like `list` returns a listing of lobbies with detail: id, num_connections,
/// 5. a meesage like `send_peer ID`

/// examples of mappings:
/// 1 client in 1 lobby:
///   1 => Arc(lobby 1)
///
/// 2 clients in 1 lobby:
///   1 => Arc(lobby 1)
///   2 => Arc(lobby 1)
///
/// 1 clients in 2 lobby: is impossible
///
/// 2 clients in 2 lobbies:
///   1 => Arc(lobby 1)
///   2 => Arc(lobby 2)

type LobbyGuard<'a> = RwLockWriteGuard<'a, Vec<Lobby, Global>>;

#[derive(Default, Debug)]
pub struct LobbyManager {
    // these fields have to be private
    connections: RwLock<Vec<Arc<Connection>>>,
    lobbies: RwLock<Vec<Lobby>>,
    mapping: RwLock<HashMap<Uuid, Uuid>>,
}

impl LobbyManager {
    pub async fn add_client(&self, conn: Arc<Connection>) {
        let mut lobby_lock = self.lobbies.write().await;
        let conn_id = conn.id;

        self.connections.write().await.push(conn.clone());

        if let Some(lobby) = lobby_lock.iter().next() {
            lobby.join(conn).await;
            self.mapping.write().await.insert(conn_id, lobby.id);
        } else {
            let lobby = Lobby::default();
            lobby.join(conn).await;
            self.mapping.write().await.insert(conn_id, lobby.id);
            lobby_lock.push(lobby);
        }
    }

    pub async fn remove_client(&self, conn_id: Uuid) {
        let lobby_id = self.mapping.write().await.remove(&conn_id).unwrap();

        if let Some(lobby) = self.lobbies.read().await.iter().find(|l| l.id == lobby_id) {
            lobby.leave(&conn_id).await;
        }

        self.connections
            .write()
            .await
            .retain(|conn| conn.id != conn_id);
    }

    pub async fn handle_message(&self, conn_id: Uuid, msg: Message) -> Result<(), Error> {
        let mut lobby_lock = self.lobbies.write().await;
        let lobby_id = self.mapping.read().await.get(&conn_id).unwrap().clone();

        if let Some(lobby) = lobby_lock.iter().find(|l| l.id == lobby_id) {
            let l_request = lobby.handle_message(msg, conn_id).await?;

            match l_request {
                lobby::LobbyRequest::None => {}
                lobby::LobbyRequest::Change { lobby_id } => {
                    self.change_lobby(conn_id, lobby_id, lobby_lock).await?;
                }
                LobbyRequest::Create { lobby_id } => {
                    self.create_lobby(lobby_id, lobby_lock)?;
                    println!("created new lobby: {}", lobby_id);
                }
                LobbyRequest::List => {
                    let msg = lobby::LobbyPacket::Message {
                        text: format!(
                            "lobbies: {:#?}",
                            lobby_lock.iter().map(|l| l.id).collect::<Vec<Uuid>>()
                        ),
                    };

                    lobby.emit(&conn_id, msg).await?;
                }
            }
        }

        Ok(())
    }

    pub fn create_lobby(&self, lobby_id: Uuid, mut lobby_lock: LobbyGuard) -> Result<(), Error> {
        let lobby = Lobby::new(lobby_id);
        lobby_lock.push(lobby);

        Ok(())
    }

    pub async fn change_lobby(
        &self,
        conn_id: Uuid,
        new_lobby: Uuid,
        mut lobby_lock: LobbyGuard<'_>,
    ) -> Result<(), Error> {
        let old_lobby = self.mapping.read().await.get(&conn_id).unwrap().clone();

        if old_lobby == new_lobby {
            println!("You are already in this lobby");
            return Ok(());
        }

        let conn = self
            .connections
            .read()
            .await
            .iter()
            .find(|c| c.id == conn_id)
            .unwrap()
            .clone();

        if let Some(lobby) = lobby_lock.iter().find(|l| l.id == old_lobby) {
            lobby.leave(&conn_id).await;
        } else {
            eprintln!("Connection dropped while trying to join new lobby");
        }

        if let Some(lobby) = lobby_lock.iter().find(|l| l.id == new_lobby) {
            lobby.join(conn).await;
        } else {
            eprintln!("Lobby doesn't exists");
        }

        self.mapping.write().await.insert(conn_id, new_lobby);

        Ok(())
    }
}

/// `handle_messages` loops over all received
async fn handle_messages(
    websocket: HyperWebsocket,
    // connection_map: Arc<RwLock<HashMap<ConnectionUudi, LobbyUuid>>> (note that lobby_uuid needs to be validated somehow)
    // lobbies: Arc<RwLock<Vec<Arc<Lobby>>>> (changes to connections in lobby need to be reflected in connection_map)
    // maybe create a struct which handles this uuid synchronization with connections and lobbies
    mapping: Arc<LobbyManager>,
) -> Result<(), Error> {
    let websocket = websocket.await?;

    let (ws_write, mut ws_read) = websocket.split();
    let ws_write = Arc::new(Mutex::new(ws_write));
    let conn_id = Uuid::new_v4();
    let conn = Arc::new(Connection::new(conn_id, ws_write));

    mapping.add_client(conn).await;

    while let Some(message) = ws_read.next().await {
        let msg = message?;

        match msg {
            Message::Text(_) | Message::Binary(_) => {
                if let Err(e) = mapping.handle_message(conn_id, msg).await {
                    eprintln!("Error occured when trying to handle message: {:#?}", e);
                }
            }
            Message::Ping(_) => todo!(),
            Message::Pong(_) => todo!(),
            Message::Frame(_) => todo!(),
            Message::Close(_) => {
                break;
            }
        }
    }

    mapping.remove_client(conn_id).await;

    Ok(())
}

async fn handle_connection(
    mut request: Request<Body>,
    mapping: Arc<LobbyManager>, // Vec<Arc<Lobby>>
) -> Result<Response<Body>, Error> {
    // dbg!(&request);
    if hyper_tungstenite::is_upgrade_request(&request) {
        let (response, websocket) = hyper_tungstenite::upgrade(&mut request, None)?;

        tokio::spawn(async move {
            let mapping = Arc::clone(&mapping);
            if let Err(e) = handle_messages(websocket, mapping).await {
                eprintln!("Error in websocket connection: {}", e);
            }
        });

        Ok(response)
    } else {
        Ok(Response::new(Body::empty()))
    }
}

#[tokio::main]
async fn main() {
    let mapping = Arc::new(LobbyManager::default());

    // The closure inside `make_service_fn` is run for each connection
    let make_service = make_service_fn(move |_| {
        let mapping = Arc::clone(&mapping);

        async move {
            Ok::<_, Infallible>(service_fn(move |request| {
                handle_connection(request, Arc::clone(&mapping))
            }))
        }
    });

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let server = Server::bind(&addr).serve(make_service);

    println!("Listening on ws://{}", addr);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}
