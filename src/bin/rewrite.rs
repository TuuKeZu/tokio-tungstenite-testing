use futures::stream::StreamExt;
use futures::SinkExt;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use hyper_tungstenite::tungstenite::Message;
use hyper_tungstenite::HyperWebsocket;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc::{self, Sender};
use tokio::sync::RwLock;
use uuid::Uuid;

type Error = Box<dyn std::error::Error + Send + Sync>;
type LobbyMessage = Message;

struct Lobby {
    id: Uuid,
}

impl Lobby {
    fn new() -> Lobby {
        Lobby { id: Uuid::new_v4() }
    }
    pub fn handle_message(&self, msg: Message, id: usize) {
        println!("Connection {id} received: {msg}");
    }
}

async fn handle_messages(
    websocket: HyperWebsocket,
    mapping: Arc<RwLock<HashMap<usize, Arc<Lobby>>>>,
    id: usize,
) -> Result<(), Error> {
    let mut websocket = websocket.await?;
    while let Some(message) = websocket.next().await {
        let message = message?;
        let text = message.to_text()?;
        if let Some(lobby) = mapping.read().await.get(&id) {
            println!(
                "[lobby {} with {} users] Connection {id} received: {text}",
                lobby.id,
                Arc::strong_count(lobby)
            );
            // websocket.send(message).await.unwrap();
            // tx.send(message).await?;
            lobby.handle_message(message, id);
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
    mapping: Arc<RwLock<HashMap<usize, Arc<Lobby>>>>,
) -> Result<Response<Body>, Error> {
    dbg!(&request);

    let mut mapping_lock = mapping.write().await;
    let id = mapping_lock.len();

    if let Some(lobby) = mapping_lock.iter().next().map(|(_, l)| Arc::clone(l)) {
        mapping_lock.insert(id, lobby);
    } else {
        mapping_lock.insert(id, Arc::new(Lobby::new()));
    };

    std::mem::drop(mapping_lock);

    if hyper_tungstenite::is_upgrade_request(&request) {
        let (response, websocket) = hyper_tungstenite::upgrade(&mut request, None)?;

        tokio::spawn(async move {
            let mapping = Arc::clone(&mapping);
            if let Err(e) = handle_messages(websocket, mapping, id).await {
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

    println!("Listening on http://{}", addr);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }

    let a = [10, 20, 30, 40];
    let b = a.to_vec();
}
