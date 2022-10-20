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
    static ref d: Arc<RwLock<u8>> = Arc::new(RwLock::new(0));
}

#[derive(Debug)]
struct WebscoketServer {
    lobbies: HashMap<u8, Lobby>,
}

impl WebscoketServer {
    fn new() -> WebscoketServer {
        WebscoketServer {
            lobbies: HashMap::from([(0, Lobby::new(0)), (1, Lobby::new(2))]),
        }
    }
    /*
    fn join_lobby(&mut self, lobby_id: u8, connection: &'a mut Connection) -> &mut Connection {
        let id = connection.id;
        connection.lobby = Some(lobby_id);

        let lobby = self.lobbies.get_mut(&lobby_id).unwrap();
        lobby.connections.insert(connection.id, connection);
        lobby.connections.get_mut(&id).unwrap()
    }
    */
}

#[derive(Debug)]
struct Lobby {
    connections: HashMap<Uuid, Connection>,
    id: u8,
    tx: UnboundedSender<Message>,
    b_tx: Sender<Message>,
}

impl Lobby {
    fn new(id: u8) -> Lobby {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut rx = UnboundedReceiverStream::new(rx);

        //
        let (b_tx, mut b_rx) = broadcast::channel(16);
        let rx2: Receiver<Message> = b_tx.subscribe();

        tokio::task::spawn(async move {
            while let Some(message) = rx.next().await {
                println!("!{:#?}", message);
            }
        });

        Lobby {
            connections: HashMap::new(),
            id,
            tx,
            b_tx,
        }
    }

    fn test(&mut self, id: Uuid, msg: Message) {
        println!("[lobby #{}] {}: {:#?}", self.id, id, msg);
    }
}
/*
    let (tx, rx) = mpsc::unbounded_channel();
    let mut rx = UnboundedReceiverStream::new(rx);
*/

#[derive(Debug)]
struct Connection {
    id: Uuid,
    lobby: Option<u8>,
    ws_write: SplitSink<WebSocketStream<Upgraded>, Message>,
    ws_read: SplitStream<WebSocketStream<Upgraded>>,
    tx: Option<UnboundedSender<Message>>,
    rx: Option<Receiver<Message>>,
}

impl Connection {
    async fn new(
        ws_write: SplitSink<WebSocketStream<Upgraded>, Message>,
        ws_read: SplitStream<WebSocketStream<Upgraded>>,
    ) -> Connection {
        let id = Uuid::new_v4();

        Connection {
            id,
            ws_write,
            ws_read,
            lobby: None,
            tx: None,
            rx: None,
        }
    }

    async fn handle_events(&mut self) {
        println!("handling events...");

        loop {
            tokio::select! {
                r = self.rx.as_mut().unwrap().recv() => {
                    match r {
                        Ok(msg) => {
                            //
                            println!("test2: {:#?}", msg)
                        },
                        Err(e) => {
                            println!("err: {:#?}", e)
                        }
                    }
                }
                s = self.ws_read.next() => {
                    match s {
                        Some(msg) => {
                            println!("test1: {:#?}", msg)
                        },
                        None => break
                    }
                }
            }
        }
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
                                let mut b = d.write().await;
                                *b += 1;

                                std::mem::drop(b);

                                tokio::spawn(async move {
                                    let ws_stream = WebSocketStream::from_raw_socket(
                                        upgraded,
                                        tokio_tungstenite::tungstenite::protocol::Role::Server,
                                        None,
                                    )
                                    .await;

                                    let (ws_write, mut ws_read) = ws_stream.split();

                                    let b = SERVER
                                        .write()
                                        .await
                                        .lobbies
                                        .get_mut(&0)
                                        .unwrap()
                                        .tx
                                        .clone();

                                    let c = SERVER
                                        .write()
                                        .await
                                        .lobbies
                                        .get_mut(&0)
                                        .unwrap()
                                        .b_tx
                                        .subscribe();

                                    let mut connection = Connection::new(ws_write, ws_read).await;
                                    connection.tx = Some(b);
                                    connection.rx = Some(c);
                                    connection.handle_events().await;
                                });

                                let a = d.read().await;

                                println!("{}", *a);

                                if *a > 1 {
                                    let t = SERVER
                                        .write()
                                        .await
                                        .lobbies
                                        .get_mut(&0)
                                        .unwrap()
                                        .b_tx
                                        .send(Message::text("user connected".to_string()));

                                    println!("{:#?}", t);
                                }
                                /*
                                SERVER
                                    .write()
                                    .await
                                    .lobbies
                                    .get_mut(&0)
                                    .unwrap()
                                    .connections
                                    .insert(id, c);
                                */
                                /*
                                server
                                    .lobbies
                                    .get_mut(&0)
                                    .unwrap()
                                    .connections
                                    .insert(connection.id, connection.borrow_mut());
                                // a.handle_events(ws_read).await;
                                */

                                /*
                                let mut server = SERVER.write().await;
                                let connection = server.join_lobby(0, connection);
                                */

                                // connection.handleEvents();
                                // SERVER.lock().await.join_lobby(0, connection);
                                //

                                /*
                                tokio::spawn(async move {
                                    let mut a = SERVER.lock().await;
                                    let b = a.lobbies.get_mut(&0).unwrap();
                                    b.handleEvents(0).await;
                                    std::mem::drop(a);

                                    // connection.handleEvents().await;
                                });
                                */

                                /*
                                //create a websocket stream from the upgraded object
                                let ws_stream = WebSocketStream::from_raw_socket(
                                    //pass the upgraded object
                                    //as the base layer stream of the Websocket
                                    upgraded,
                                    tokio_tungstenite::tungstenite::protocol::Role::Server,
                                    None,
                                )
                                .await;

                                //we can split the stream into a sink and a stream
                                let (mut ws_write, mut ws_read) = ws_stream.split();

                                while let Some(msg) = ws_read.next().await {
                                    let msg = msg.unwrap();
                                    if msg.is_text() || msg.is_binary() {
                                        println!("{:#?}", msg);
                                        ws_write.send(msg).await;
                                    }
                                }

                                //forward the stream to the sink to achieve echo

                                match ws_read.forward(ws_write).await {
                                    Ok(_) => {}
                                    Err(Error::ConnectionClosed) => {
                                        println!("Connection closed normally")
                                    }
                                    Err(e) => println!(
                                        "error creating echo stream on \
                                                    connection from address {}. \
                                                    Error is {}",
                                        remote_addr, e
                                    ),
                                };
                                */
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
