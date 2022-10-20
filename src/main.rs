use futures::select;
use futures::stream::{SplitSink, SplitStream, StreamExt};
use futures::{FutureExt, SinkExt};
use hyper::service::{make_service_fn, service_fn};
use hyper::upgrade::Upgraded;
use hyper::{
    header, server::conn::AddrStream, upgrade, Body, Request, Response, Server, StatusCode,
};
use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::convert::Infallible;
use std::hash::Hash;
use std::net::SocketAddr;
use std::sync::Arc;
use std::u8;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::{broadcast, Mutex};
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::WebSocketStream;
use tungstenite::{handshake, Error, Message};

use uuid::Uuid;

#[macro_use]
extern crate lazy_static;

lazy_static! {
    static ref SERVER: Arc<RwLock<WebscoketServer>> = Arc::new(RwLock::new(WebscoketServer::new()));
    static ref CONNECTIONS: Arc<RwLock<HashMap<Uuid, Connection>>> =
        Arc::new(RwLock::new(HashMap::new()));
}

#[derive(Debug)]
struct WebscoketServer {
    lobbies: HashMap<u8, Lobby>,
}

impl WebscoketServer {
    fn new() -> WebscoketServer {
        let lobby0 = Lobby::new(0);
        let lobby1 = Lobby::new(1);

        WebscoketServer {
            lobbies: HashMap::from([(0, lobby0), (1, lobby1)]),
        }
    }
}

#[derive(Debug)]
struct Lobby {
    connections: HashMap<Uuid, UnboundedSender<Message>>,
    id: u8,
    tx: UnboundedSender<Message>,
    rx: UnboundedReceiverStream<Message>,
}

impl Lobby {
    fn new(id: u8) -> Lobby {
        let (tx, rx) = mpsc::unbounded_channel();
        let rx = UnboundedReceiverStream::new(rx);

        /*
        useless as the self cannot be borrowed as mutable
        ***
        tokio::task::spawn(async move {
            while let Some(message) = rx.next().await {
                println!("[{}] {:#?}", id, message);
            }
        });
        */

        Lobby {
            connections: HashMap::new(),
            id,
            tx,
            rx,
        }
    }

    async fn handle_events(&mut self) {
        while let Some(message) = self.rx.next().await {
            println!("[{}] {:#?}", self.id, message);
            self.borrow_mut();
        }
    }

    fn join(&mut self, id: &Uuid, tx: UnboundedSender<Message>) -> UnboundedSender<Message> {
        self.connections.insert(*id, tx);
        println!("[{}] User has joined the lobby", self.id);
        self.tx.clone()
    }

    fn leave(&mut self, id: &Uuid) {
        self.connections.remove(id);
        println!("[{}] User has left the lobby", self.id);
    }

    fn broadcast(&mut self, msg: Message) {
        self.connections.iter_mut().for_each(|(id, connection)| {
            connection.send(msg.clone());
        });
    }
}

#[derive(Debug)]
struct Connection {
    id: Uuid,
    lobby: Option<u8>,
    ws_write: SplitSink<WebSocketStream<Upgraded>, Message>,
    ws_read: SplitStream<WebSocketStream<Upgraded>>,
    tx: UnboundedSender<Message>,
    rx: UnboundedReceiverStream<Message>,
    lobby_tx: Option<UnboundedSender<Message>>,
}

impl Connection {
    async fn new(
        ws_write: SplitSink<WebSocketStream<Upgraded>, Message>,
        ws_read: SplitStream<WebSocketStream<Upgraded>>,
    ) -> Connection {
        let id = Uuid::new_v4();
        let (tx, rx) = mpsc::unbounded_channel();
        let rx = UnboundedReceiverStream::new(rx);

        Connection {
            id,
            ws_write,
            ws_read,
            lobby: None,
            tx,
            rx,
            lobby_tx: None,
        }
    }

    async fn move_to_lobby(&mut self, lobby_id: &u8) {
        if self.lobby.is_some() {
            self.lobby_tx = None;
            let id = self.lobby.unwrap();

            SERVER
                .write()
                .await
                .lobbies
                .get_mut(&id)
                .unwrap()
                .leave(&self.id);
        }

        let lobby_tx = SERVER
            .write()
            .await
            .lobbies
            .get_mut(lobby_id)
            .unwrap()
            .join(&self.id, self.tx.clone());

        self.lobby = Some(*lobby_id);
        self.lobby_tx = Some(lobby_tx);
    }

    async fn handle_events(&mut self) {
        println!("handling events...");

        loop {
            tokio::select! {
                r = self.rx.as_mut().recv() => {
                    match r {
                        Some(msg) => {

                            println!("{}: {:#?}", self.id, msg);
                            // forward messages from lobby to websocket
                            self.ws_write.send(msg).await;
                        },
                        None => break
                    }
                }
                s = self.ws_read.next() => {
                    match s {
                        Some(r) => {
                            match r {
                                Ok(msg) => {
                                    if msg.to_string() == "move" {
                                        self.move_to_lobby(&1).await;
                                    }
                                    else {
                                        // forward the websocket message to current lobby_tx
                                        self.lobby_tx.as_mut().unwrap().send(msg.clone());
                                    }
                                },
                                Err(e) => {
                                    //
                                    println!("err: {:#?}", e);
                                }
                            }

                        },
                        None => break
                    }
                }
            }
        }
        println!("handled disconnect");
        self.lobby_tx
            .as_mut()
            .unwrap()
            .send(Message::Text("client has disconnected".to_string()));
    }
}

async fn handle_request(
    mut request: Request<Body>,
    remote_addr: SocketAddr,
) -> Result<Response<Body>, Infallible> {
    match (
        request.uri().path(),
        request.headers().contains_key(header::UPGRADE),
    ) {
        //if the request is ws_echo and the request headers contains an Upgrade key
        ("/ws_echo", true) => {
            //assume request is a handshake, so create the handshake response
            let response = match handshake::server::create_response_with_body(&request, || {
                Body::empty()
            }) {
                Ok(response) => {
                    tokio::spawn(async move {
                        match upgrade::on(&mut request).await {
                            Ok(upgraded) => {
                                tokio::spawn(async move {
                                    let ws_stream = WebSocketStream::from_raw_socket(
                                        upgraded,
                                        tokio_tungstenite::tungstenite::protocol::Role::Server,
                                        None,
                                    )
                                    .await;

                                    let (ws_write, ws_read) = ws_stream.split();

                                    let mut connection = Connection::new(ws_write, ws_read).await;
                                    connection.move_to_lobby(&2).await;
                                    connection.handle_events().await;
                                });
                            }
                            Err(e) => println!(
                                "error when trying to upgrade connection \
                                        from address {} to websocket connection. \
                                        Error is: {}",
                                remote_addr, e
                            ),
                        }
                    });
                    //return the response to the handshake request
                    response
                }
                Err(error) => {
                    //probably the handshake request is not up to spec for websocket
                    println!(
                        "Failed to create websocket response \
                                to request from address {}. \
                                Error is: {}",
                        remote_addr, error
                    );
                    let mut res =
                        Response::new(Body::from(format!("Failed to create websocket: {}", error)));
                    *res.status_mut() = StatusCode::BAD_REQUEST;
                    return Ok(res);
                }
            };

            Ok::<_, Infallible>(response)
        }
        ("/ws_echo", false) => {
            //handle the case where the url is /ws_echo, but does not have an Upgrade field
            Ok(Response::new(Body::from(format!(
                "Getting even warmer, \
                                                try connecting to this url \
                                                using a websocket client.\n"
            ))))
        }
        (url @ _, false) => {
            //handle any other url without an Upgrade header field
            Ok(Response::new(Body::from(format!(
                "This {} url doesn't do \
                                                much, try accessing the \
                                                /ws_echo url instead.\n",
                url
            ))))
        }
        (_, true) => {
            //handle any other url with an Upgrade header field
            Ok(Response::new(Body::from(format!(
                "Getting warmer, but I'm \
                                                only letting you connect \
                                                via websocket over on \
                                                /ws_echo, try that url.\n"
            ))))
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    //hyper server boilerplate code from https://hyper.rs/guides/server/hello-world/

    // We'll bind to 127.0.0.1:3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    println!("Listening on {} for http or websocket connections.", addr);

    // A `Service` is needed for every connection, so this
    // creates one from our `handle_request` function.
    let make_svc = make_service_fn(|conn: &AddrStream| {
        let remote_addr = conn.remote_addr();
        async move {
            // service_fn converts our function into a `Service`
            Ok::<_, Infallible>(service_fn(move |request: Request<Body>| {
                handle_request(request, remote_addr)
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_svc);

    // Run this server for... forever!
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}
