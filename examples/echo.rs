/*!
Simple echo server implementation.
*/

#![allow(unused_variables)]

use std::{io, net, thread};

pub struct Connection {
	pub stream: net::TcpStream,
	pub addr: net::SocketAddr,
	pub ws: websock::WebSocket<websock::VecStrategy>,
	pub cl: EchoClient,
}
impl Connection {
	pub fn new(stream: net::TcpStream, addr: net::SocketAddr) -> Connection {
		Connection {
			stream,
			addr,
			ws: websock::WebSocket::server(websock::VecStrategy { capacity: 1000 }),
			cl: EchoClient::default(),
		}
	}
}

#[derive(Default)]
pub struct EchoClient {
	pub closing: bool,
}
impl websock::Handler for EchoClient {
	fn http_request(&mut self, method: &str, uri: &str) -> Result<(), websock::Error> {
		// MUST be GET request
		if method != "GET" {
			return Err(websock::Error::METHOD_NOT_ALLOWED);
		}

		// Check supported URIs
		// This should probably use a proper URI parser
		if uri.starts_with("/echo") {
			return Ok(())
		}

		Err(websock::Error::NOT_FOUND)
	}

	fn message(&mut self, ws: &mut websock::WebSocketTx, msg: websock::Msg) {
		match msg {
			websock::Msg::Text(text) => {
				#[cfg(feature = "debug-print")]
				println!("Msg::Text({:?})", text);

				ws.send_text(text, true);
			},
			websock::Msg::Binary(data) => {
				#[cfg(feature = "debug-print")]
				println!("Msg::Binary({} bytes)", data.len());
			},
			websock::Msg::Close(close, reason) => {
				#[cfg(feature = "debug-print")]
				println!("Msg::Close({}, {:?})", close, reason);

				ws.send_close(close, reason);
				self.closing = true;
			},
			websock::Msg::Ping(payload) => {
				#[cfg(feature = "debug-print")]
				println!("Msg::Ping({} bytes)", payload.len());

				ws.send_pong(payload);
			},
			websock::Msg::Pong(payload) => {
				#[cfg(feature = "debug-print")]
				println!("Msg::Pong({} bytes)", payload.len());
			},
		}
	}
}

fn main() {
	let listener = net::TcpListener::bind((net::Ipv4Addr::UNSPECIFIED, 30147)).unwrap();

	while let Ok((stream, addr)) = listener.accept() {
		#[cfg(feature = "debug-print")]
		println!("accept(): {}", addr);

		thread::spawn(move || {
			let mut conn = Connection::new(stream, addr);

			loop {
				// Read incoming network traffic into the buffer
				match conn.ws.recv(&mut conn.stream) {
					Ok(n) => {
						#[cfg(feature = "debug-print")]
						println!("recv(): {} bytes!", n);

						if n == 0 {
							#[cfg(feature = "debug-print")]
							println!("abort: {}", conn.addr);

							return Ok(());
						}
					},
					Err(err) => {
						if err.kind() != io::ErrorKind::WouldBlock {
							#[cfg(feature = "debug-print")]
							println!("recv(): {}", err);

							return Err(err);
						}
					},
				}

				// Parse incoming data and dispatch handlers
				match conn.ws.dispatch(&mut conn.cl) {
					Ok(()) => (),
					Err(err) => {
						#[cfg(feature = "debug-print")]
						println!("dispatch(): {:?}", err);

						return Err(io::ErrorKind::InvalidData.into());
					},
				};

				// Transmit outgoing data
				conn.ws.send_all(&mut conn.stream)?;

				// The connection is closing
				if conn.cl.closing {
					#[cfg(feature = "debug-print")]
					println!("closing: {}", conn.addr);

					return Ok(());
				}
			}
		});
	}
}
