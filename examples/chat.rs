/*!
Simple chat server implementation.
*/

use std::{io, net, thread};
use std::sync::{Arc, Mutex};

pub struct ChatServer {}

pub struct Connection {
	pub stream: net::TcpStream,
	pub addr: net::SocketAddr,
	pub ws: websock::WebSocket<websock::VecStrategy>,
	pub cl: ChatClient,
}
impl Connection {
	pub fn new(stream: net::TcpStream, addr: net::SocketAddr, server: Arc<Mutex<ChatServer>>) -> Connection {
		Connection {
			stream,
			addr,
			ws: websock::WebSocket::server(websock::VecStrategy { capacity: 1000 }),
			cl: ChatClient {
				closing: false,
				server,
			},
		}
	}
}

pub struct ChatClient {
	pub closing: bool,
	pub server: Arc<Mutex<ChatServer>>,
}
impl websock::Handler for ChatClient {
	fn http_request(&mut self, method: &str, uri: &str) -> Result<(), websock::Error> {
		println!("{:?} {:?}", method, uri);

		// MUST be GET request
		if method != "GET" {
			return Err(websock::Error::HttpClientError(5)); // 405
		}

		// Check supported URIs
		// This should probably use a proper URI parser
		if uri.starts_with("/chat") {
			return Ok(())
		}

		Err(websock::Error::HttpServerError(4)) // 404
	}

	fn message(&mut self, ws: &mut websock::WebSocketTx, msg: websock::Msg) {
		match msg {
			websock::Msg::Text(text) => {
				println!("Text: {:?}", text);
				ws.send_text(text, true);
			},
			websock::Msg::Close(close, reason) => {
				ws.send_close(close, reason);
				self.closing = true;
			},
			websock::Msg::Ping(payload) => {
				ws.send_pong(payload);
			},
			_ => (),
		}
	}
}

fn main() {
	let listener = net::TcpListener::bind((net::Ipv4Addr::UNSPECIFIED, 30147)).unwrap();
	let server = Arc::new(Mutex::new(ChatServer {}));

	while let Ok((stream, addr)) = listener.accept() {
		let server = server.clone();
		thread::spawn(move || {
			let mut conn = Connection::new(stream, addr, server);

			loop {
				// Read incoming network traffic into the buffer
				match conn.ws.recv(&mut conn.stream) {
					Ok(n) => {
						if n == 0 {
							return Ok(());
						}
					},
					Err(err) => {
						if err.kind() != io::ErrorKind::WouldBlock {
							return Err(err);
						}
					},
				}

				// Parse incoming data and dispatch handlers
				match conn.ws.dispatch(&mut conn.cl) {
					Ok(()) => (),
					Err(_err) => {
						return Err(io::ErrorKind::InvalidData.into());
					},
				};

				// Transmit outgoing data
				conn.ws.send_all(&mut conn.stream)?;

				// The connection is closing
				if conn.cl.closing {
					println!("{} closing", conn.addr);
					return Ok(());
				}
			}
		});
	}
}
