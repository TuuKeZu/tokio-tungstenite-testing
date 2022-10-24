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

type Error = Box<dyn std::error::Error + Send + Sync>;
type LobbyMessage = Message;

async fn handle_messages(
    websocket: HyperWebsocket,
    mapping: Arc<RwLock<HashMap<usize, Sender<LobbyMessage>>>>,
    id: usize,
) -> Result<(), Error> {
    let mut websocket = websocket.await?;
    while let Some(message) = websocket.next().await {
        let message = message?;
        let text = message.to_text()?;
        println!("Connection {id} received: {text}");
        if let Some(tx) = mapping.read().await.get(&id) {
            // websocket.send(message).await.unwrap();
            tx.send(message).await?;
        }
    }
    Ok(())
}

async fn handle_connection(
    mut request: Request<Body>,
    mapping: Arc<RwLock<HashMap<usize, Sender<LobbyMessage>>>>,
    lobby_mapping: Arc<RwLock<HashMap<usize, Sender<LobbyMessage>>>>,
) -> Result<Response<Body>, Error> {
    dbg!(&request);

    let mut lobby_lock = lobby_mapping.write().await;
    let mut mapping_lock = mapping.write().await;
    let id = mapping_lock.len();

    // unwrap for now
    let lobby_sender = lobby_lock.get_mut(&0).unwrap().clone();

    mapping_lock.insert(id, lobby_sender);

    std::mem::drop(lobby_lock);
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
    let lobby_mapping = Arc::new(RwLock::new(HashMap::new()));

    // initialize lobbies
    let mut lobby_lock = lobby_mapping.write().await;
    for i in 0..4 {
        println!("created lobby #{}", i);
        let (sender, mut receiver) = mpsc::channel(1); // Unbounded channel is

        lobby_lock.insert(i, sender);

        tokio::spawn(async move {
            while let Some(msg) = receiver.recv().await {
                println!("[Lobby {}] received message: {}", i, msg);
            }
        });
    }

    std::mem::drop(lobby_lock);

    // The closure inside `make_service_fn` is run for each connection
    let make_service = make_service_fn(move |_| {
        let mapping = Arc::clone(&mapping);
        let lobby_mapping = Arc::clone(&lobby_mapping);

        async move {
            Ok::<_, Infallible>(service_fn(move |request| {
                handle_connection(request, Arc::clone(&mapping), Arc::clone(&lobby_mapping))
            }))
        }
    });

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let server = Server::bind(&addr).serve(make_service);

    println!("Listening on http://{}", addr);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}
