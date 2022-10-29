mod lobby;

use futures::stream::{SplitSink, StreamExt};
use futures::SinkExt;
use hyper::service::{make_service_fn, service_fn};
use hyper::upgrade::Upgraded;
use hyper::{Body, Request, Response, Server};
use hyper_tungstenite::tungstenite::Message;
use hyper_tungstenite::{HyperWebsocket, WebSocketStream};
use lobby::{ChangeLobbyMessage, Connection, Lobby};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
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

pub struct LobbyManager {
    // these fields have to be private
    connections: RwLock<Vec<Connection>>,
    lobbies: RwLock<Vec<Lobby>>,
    mapping: RwLock<HashMap<Uuid, Uuid>>,
}

impl LobbyManager {
    pub fn change_lobby(&mut self, conn_id: Uuid) -> Result<(), Error> {
        todo!()
    }

    pub fn remove_client(&self, conn_id: Uuid) {
        todo!()
    }

    pub fn handle_message(&self, conn_id: Uuid, msg: Message) {
        todo!()
    }
}

/// `handle_messages` loops over all received
async fn handle_messages(
    websocket: HyperWebsocket,
    // connection_map: Arc<RwLock<HashMap<ConnectionUudi, LobbyUuid>>> (note that lobby_uuid needs to be validated somehow)
    // lobbies: Arc<RwLock<Vec<Arc<Lobby>>>> (changes to connections in lobby need to be reflected in connection_map)
    // maybe create a struct which handles this uuid synchronization with connections and lobbies
    mapping: Arc<RwLock<HashMap<Uuid, Arc<Lobby>>>>,
) -> Result<(), Error> {
    let websocket = websocket.await?;

    let (ws_write, mut ws_read) = websocket.split();
    let ws_write = Arc::new(Mutex::new(ws_write));

    let id = Uuid::new_v4();
    let lobby = {
        let mut mapping_lock = mapping.write().await;

        if let Some(lobby) = mapping_lock.iter().next().map(|(_, l)| Arc::clone(l)) {
            mapping_lock.insert(id, Arc::clone(&lobby));
            lobby
        } else {
            let lobby = Arc::new(Lobby::new());
            mapping_lock.insert(id, Arc::clone(&lobby));
            lobby
        }
    };

    lobby.join(id, Arc::clone(&ws_write)).await;

    while let Some(message) = ws_read.next().await {
        let msg = message?;

        if let Some(lobby) = mapping.read().await.get(&id) {
            match msg.clone() {
                Message::Text(_) | Message::Close(_) => {
                    let lobby_change = lobby.handle_message(msg, id).await?;

                    if let ChangeLobbyMessage::Change(new_lobby_id) = lobby_change {
                        if let Some(new_lobby) = mapping
                            .read()
                            .await
                            .iter()
                            .find(|(_id, lobby)| lobby.id == new_lobby_id)
                            .map(|(_, new_lobby)| Arc::clone(new_lobby))
                        {
                            // This needs to be refactor ed heavily
                            let mut mapping_lock = mapping.write().await;
                            let old_lobby = mapping_lock.remove(&id).unwrap();
                            old_lobby.leave(&id).await;
                            mapping_lock.insert(id, Arc::clone(&new_lobby));
                            new_lobby.join(id, Arc::clone(&ws_write)).await;
                        } else {
                            eprintln!("Lobby tried to change connection to nonexistent lobby");
                            // should not panic?
                        }
                    }
                }
                Message::Binary(_) => todo!(),
                Message::Ping(_) => println!("poing"),
                Message::Pong(_) => todo!(),
                Message::Frame(_) => todo!(),
            }
        } else {
            unreachable!()
        }
    }

    let mut mapping_lock = mapping.write().await;
    mapping_lock.remove(&id);

    Ok(())
}

async fn handle_connection(
    mut request: Request<Body>,
    mapping: Arc<RwLock<HashMap<Uuid, Arc<Lobby>>>>, // Vec<Arc<Lobby>>
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
    let mapping = Arc::new(RwLock::new(HashMap::new()));

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
