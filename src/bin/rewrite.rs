use futures::stream::StreamExt;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use hyper_tungstenite::HyperWebsocket;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc::{self, Sender};
use tokio::sync::RwLock;

type Error = Box<dyn std::error::Error + Send + Sync>;
type LobbyMessage = usize;

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
            tx.send(id).await?;
        }
    }
    Ok(())
}

async fn handle_connection(
    mut request: Request<Body>,
    mapping: Arc<RwLock<HashMap<usize, Sender<LobbyMessage>>>>,
) -> Result<Response<Body>, Error> {
    dbg!(&request);

    let (sender, mut receiver) = mpsc::channel(1); // Unbounded channel is better

    let mut mapping_lock = mapping.write().await;
    let id = mapping_lock.len();
    mapping_lock.insert(id, sender.clone());
    drop(mapping_lock);

    if hyper_tungstenite::is_upgrade_request(&request) {
        let (response, websocket) = hyper_tungstenite::upgrade(&mut request, None)?;

        tokio::spawn(async move {
            let mapping = Arc::clone(&mapping);
            if let Err(e) = handle_messages(websocket, mapping, id).await {
                eprintln!("Error in websocket connection: {}", e);
            }
        });

        tokio::spawn(async move {
            while let Some(msg) = receiver.recv().await {
                println!("Lobby: received message from connection id {msg}");
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
}
