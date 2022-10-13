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
use std::sync::Mutex;
use tokio_tungstenite::WebSocketStream;
use tungstenite::{handshake, Error, Message};

#[macro_use]
extern crate lazy_static;

lazy_static! {
    static ref LOBBIES: Mutex<HashMap<u8, Lobby>> =
        Mutex::new(HashMap::from([(0, Lobby::new()), (1, Lobby::new())]));
}

#[derive(Debug)]
struct Lobby {
    connections: Vec<Connection>,
}

impl Lobby {
    fn new() -> Lobby {
        Lobby {
            connections: Vec::new(),
        }
    }
}

#[derive(Debug)]
struct Connection {
    sender: SplitSink<WebSocketStream<Upgraded>, Message>,
    receiver: SplitStream<WebSocketStream<Upgraded>>,
    lobby: Option<Lobby>,
}

impl Connection {
    async fn new(upgraded: Upgraded) -> Connection {
        let ws_stream = WebSocketStream::from_raw_socket(
            upgraded,
            tokio_tungstenite::tungstenite::protocol::Role::Server,
            None,
        )
        .await;

        let (ws_write, ws_read) = ws_stream.split();

        Connection {
            sender: ws_write,
            receiver: ws_read,
            lobby: None,
        }
    }

    async fn joinLobby(mut self) {
        LOBBIES
            .lock()
            .unwrap()
            .get_mut(&0)
            .unwrap()
            .connections
            .push(self);
    }

    async fn handleEvents(&mut self) {
        while let Some(msg) = self.receiver.next().await {
            let msg = msg.unwrap();
            if msg.is_text() || msg.is_binary() {
                println!("{:#?}", msg);
                self.sender.send(msg).await;
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
                                println!("{:#?}", LOBBIES.lock().unwrap());
                                let connection = Connection::new(upgraded).await;
                                connection.joinLobby().await;

                                println!("{:#?}", LOBBIES.lock().unwrap());

                                // connection.handleEvents().await;

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
