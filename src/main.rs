#![feature(allocator_api)]
#![feature(associated_type_defaults)]

use lib::connection::*;
use lib::lobbies::lobby_default::LobbyDefault;
use lib::lobbies::lobby_default::ServerPacket;
//use lib::lobbies::lobby_chat::LobbyChat;
use lib::lobby::*;

use futures::stream::StreamExt;
use futures::SinkExt;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use hyper_tungstenite::tungstenite::Message;
use hyper_tungstenite::HyperWebsocket;

use std::alloc::Global;
use std::collections::HashMap;
use std::convert::Infallible;

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, RwLockWriteGuard};
use uuid::Uuid;

use colored::*;

type Error = Box<dyn std::error::Error + Send + Sync>;

/// situation for LobbyManager explained:
/// - method `handle_messages`
///     - holds read-lock for `self.lobbies`
///     - might call `change_lobby` or `create_lobby`
///     - both of which need a write-lock to `self.lobbies`
/// - therefore, `handle_messages` should hold write_lock, and pass the `lobby_lock` (or `LobbyGuard`) for
/// the methods that require write-lock. Little messy, but is the most ergonomic way to handle lobby events
type LobbyGuard<'a> = RwLockWriteGuard<'a, Vec<Box<dyn Lobby, Global>>>;


#[derive(Default)]
pub struct LobbyManager {
    // these fields should be private
    connections: RwLock<Vec<Arc<Connection>>>,
    lobbies: RwLock<Vec<Box<dyn Lobby>>>,
    mapping: RwLock<HashMap<Uuid, Uuid>>,
}

impl LobbyManager {
    pub async fn add_client(&self, conn: Arc<Connection>) -> Result<(), Error> {
        let mut lobby_lock = self.lobbies.write().await;
        let conn_id = conn.id;

        self.connections.write().await.push(conn.clone());

        if let Some(lobby) = lobby_lock.iter().find(|l| l.is_default()) {
            lobby.join(conn).await?;
            self.mapping.write().await.insert(conn_id, lobby.get_id());
        } else {
            let lobby = LobbyDefault::new(Uuid::new_v4());
            lobby.initialize();
            lobby.join(conn).await?;
            self.mapping.write().await.insert(conn_id, lobby.get_id());
            lobby_lock.push(Box::new(lobby));
        }

        Ok(())
    }

    pub async fn get_connection(&self, conn_id: Uuid) -> Option<Arc<Connection>> {
        self.connections
            .read()
            .await
            .iter()
            .find(|c| c.id == conn_id)
            .cloned()
    }

    async fn emit(&self, conn_id: Uuid, msg: Message) -> Result<(), Error> {
        if let Some(conn) = self.get_connection(conn_id).await {
            conn.sink.lock().await.send(msg).await?;
        }
        Ok(())
    }

    pub async fn remove_client(&self, conn_id: Uuid) -> Result<(), Error> {
        let mut lobby_lock = self.lobbies.write().await;
        let lobby_id = self.mapping.write().await.remove(&conn_id).unwrap();

        if let Some(lobby) = lobby_lock.iter().find(|l| l.get_id() == lobby_id) {
            lobby.leave(&conn_id).await?;
                
            if lobby.is_empty().await {
                println!("{} {}", lobby, format!("Dropped").red().bold());

                lobby_lock.retain(|l| l.get_id() != lobby_id);
            }
        }

        self.connections
            .write()
            .await
            .retain(|conn| conn.id != conn_id);
        
        Ok(())
    }

    pub async fn handle_message(&self, conn_id: Uuid, msg: Message) -> Result<(), Error> {
        let lobby_id = *self.mapping.read().await.get(&conn_id).unwrap();
        
        let l_request = if let Some(lobby) = self.lobbies.read().await.iter().find(|l| l.get_id() == lobby_id){
            lobby.handle_message(msg, conn_id).await
        } else {
            return Ok(())
        };

        let mut lobby_lock = self.lobbies.write().await;
        match l_request {
            Ok(l_request) => {
                self.handle_lobby_request(conn_id, &mut lobby_lock, l_request)
                .await?
            }
            Err(e) => {
                self.emit(conn_id, ServerPacket::Error { err: e.to_string() }.try_into()?).await?;
            }
        }

        Ok(())
    }

    pub async fn handle_lobby_request(
        &self,
        conn_id: Uuid,
        lobby_lock: &mut LobbyGuard<'_>,
        l_request: LobbyRequest,
    ) -> Result<(), Error> {
        match l_request {
            LobbyRequest::None => Ok(()),
            LobbyRequest::Change { lobby_id } => {
                self.change_lobby(conn_id, lobby_id, lobby_lock).await?;
                Ok(())
            }
            LobbyRequest::Create { lobby_id } => {
                let lobby_id = self.create_lobby(lobby_id, lobby_lock)?;
                self.change_lobby(conn_id, lobby_id, lobby_lock).await?;
                Ok(())
            }
            LobbyRequest::List => {
                let lobby_list: Vec<String> = lobby_lock
                    .iter()
                    .map(|l| format!("{}", l))
                    .collect();

                let msg = ServerPacket::Message {
                    text: format!("{:#?}", lobby_list),
                }.try_into()?;

                self.emit(conn_id, msg).await?;
                Ok(())
            }
        }
    }

    pub fn create_lobby(&self, lobby_id: Uuid, lobby_lock: &mut LobbyGuard) -> Result<Uuid, Error> {
        let lobby = LobbyDefault::new(lobby_id);
        lobby.initialize();
        lobby_lock.push(Box::new(lobby));

        Ok(lobby_id)
    }

    pub async fn change_lobby(
        &self,
        conn_id: Uuid,
        new_lobby_id: Uuid,
        lobby_lock: &mut LobbyGuard<'_>,
    ) -> Result<(), Error> {
        let old_lobby_id = *self.mapping.read().await.get(&conn_id).unwrap();

        if old_lobby_id == new_lobby_id {
            println!("You are already in this lobby");
            return Ok(());
        }
        
        if let Some(lobby) = lobby_lock.iter().find(|l| l.get_id() == old_lobby_id) {
            lobby.leave(&conn_id).await?;

            if lobby.is_empty().await {
                println!("{} {}", lobby, format!("Dropped").red().bold());

                lobby_lock.retain(|l| l.get_id() != old_lobby_id);
            }
        }
        
        let conn = self.get_connection(conn_id).await.unwrap();
        
        if let Some(lobby) = lobby_lock.iter().find(|l| l.get_id() == new_lobby_id) {
            lobby.join(conn).await?;
        } else {
            eprintln!("Lobby doesn't exists");
        }

        self.mapping.write().await.insert(conn_id, new_lobby_id);

        Ok(())
    }
}

/// `handle_messages` loops over all received
async fn handle_messages(
    websocket: HyperWebsocket,
    mapping: Arc<LobbyManager>,
) -> Result<(), Error> {
    let websocket = websocket.await?;

    let (ws_write, mut ws_read) = websocket.split();
    let ws_write = Arc::new(Mutex::new(ws_write));
    let conn_id = Uuid::new_v4();
    let conn = Arc::new(Connection::new(conn_id, ws_write));

    mapping.add_client(conn).await?;

    while let Some(message) = ws_read.next().await {
        let msg = message?;

        match msg {
            Message::Text(_) | Message::Binary(_) => {
                if let Err(e) = mapping.handle_message(conn_id, msg).await {
                    eprintln!("Error occured when trying to handle message: {}", format!("{:#?}", e).red());
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

    mapping.remove_client(conn_id).await?;

    Ok(())
}

async fn handle_connection(
    mut request: Request<Body>,
    mapping: Arc<LobbyManager>,
) -> Result<Response<Body>, Error> {
    if hyper_tungstenite::is_upgrade_request(&request) {
        let (response, websocket) = hyper_tungstenite::upgrade(&mut request, None)?;

        tokio::spawn(async move {
            let mapping = Arc::clone(&mapping);
            if let Err(e) = handle_messages(websocket, mapping).await {
                eprintln!("Error in websocket connection: {}", format!("{}", e).red());
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

    println!("Server started on {}", format!("ws://{}", addr).green());


    if let Err(e) = server.await {
        eprintln!("Server error: {}", format!("{}", e).red());
    }
}
