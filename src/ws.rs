use core::{result, str};
#[cfg(feature = "std")]
use std::io;
use crate::*;

impl<B: BufferStrategy> WebSocket<B> {
	/// Creates a new WebSocket server starting from a HTTP upgrade request.
	pub fn server(strategy: B) -> WebSocket<B> {
		WebSocket::server_with(strategy.new_rx(), strategy.new_tx(), strategy.new_msg())
	}

	/// Creates a new WebSocket server starting from a HTTP upgrade request.
	pub fn server_with(rxbuffer: B::RxBuffer, txbuffer: B::TxBuffer, payload: B::MsgBuffer) -> WebSocket<B> {
		WebSocket {
			inner: WebSocketInner { txbuffer },
			rxbuffer,
			stm: crate::ConnSTM::SERVER,
			handshake: http::Handshake::new(),
			payload,
		}
	}

	/*
	/// Creates a new WebSocket server starting from a HTTP client response.
	pub fn client(strategy: B, method: &str, uri: &str) -> WebSocket<B> {
		let rxbuffer = strategy.new_rx();
		let txbuffer = strategy.new_tx();
		let payload = strategy.new_msg();
		let mut ws = WebSocket {
			inner: WebSocketInner { strategy, txbuffer },
			rxbuffer,
			stm: crate::ConnSTM::CLIENT,
			handshake: http::Handshake::new(),
			payload,
		};
		ws.send_request(method, uri);
		ws
	}
	*/

	/// Creates a raw WebSocket, skipping the HTTP bits.
	pub fn raw(strategy: B) -> WebSocket<B> {
		let rxbuffer = strategy.new_rx();
		let txbuffer = strategy.new_tx();
		let payload = strategy.new_msg();
		WebSocket {
			inner: WebSocketInner { txbuffer },
			rxbuffer,
			stm: crate::ConnSTM::RAW,
			handshake: http::Handshake::new(),
			payload,
		}
	}
}

impl<B: BufferStrategy> WebSocket<B> {
	/// Returns if the state of this WebSocket has passed its lifetime as a http request.
	pub fn is_ready(&self) -> bool {
		matches!(self.stm, ConnSTM::WebSocket(_))
	}

	/// Returns the transmit interface if this WebSocket is ready.
	pub fn transmit(&mut self) -> Option<&mut WebSocketTx> {
		if !self.is_ready() {
			return None;
		}
		Some(self.inner.as_tx())
	}

	/// Fill the recv buffer with data from the IO reader.
	#[cfg(feature = "std")]
	pub fn recv(&mut self, read: &mut dyn io::Read) -> io::Result<usize> {
		let buf = self.rxbuffer.spare();
		let n = read.read(buf)?;
		self.rxbuffer.add_len(n);
		Ok(n)
	}

	/// Fill the recv buffer with data from the callback.
	pub fn recv_fn<F, E>(&mut self, mut f: F) -> result::Result<usize, E> where F: FnMut(&mut [u8]) -> result::Result<usize, E> {
		let buf = self.rxbuffer.spare();
		let n = f(buf)?;
		self.rxbuffer.add_len(n);
		Ok(n)
	}

	/// After filling the recv buffer parse the data stream and dispatch event handlers.
	pub fn dispatch(&mut self, handler: &mut dyn Handler) -> Result<()> {
		// Satisfy the borrow checker by temporarily swapping out the rxbuffer
		// Any mutations to the rxbuffer will be lost while this process is active
		// Any panics will lose the rxbuffer and corrupt the websocket state
		let mut rxbuffer = self.rxbuffer.take();
		let buffer = rxbuffer.getx_mut();

		// The current offset in the rxbuffer we've parsed so far
		let mut offset = 0;

		// Extract all the possible data from the rxbuffer until it's either empty or contains an incomplete event
		let result = loop {
			// The data in the rxbuffer takes different shapes depending on the state of the websocket connection
			let buffer = &mut buffer[offset..];
			let result = match self.stm {
				ConnSTM::HttpRequest => http::parse_request(buffer, self, handler).map_err(|err| http::error(self, err)),
				ConnSTM::HttpResponse => http::parse_response(buffer, self, handler),
				ConnSTM::HttpHeader => http::parse_header(buffer, self, handler).map_err(|err| http::error(self, err)),
				ConnSTM::WebSocket(ws_rx) => frame::handle_frame(buffer, ws_rx, self, handler),
				ConnSTM::Error(err) => Err(err),
			};

			match result {
				Ok(read) => {
					offset += read;
				},
				Err(Error::WouldBlock) => {
					break Ok(());
				},
				Err(err) => {
					// Get permanently stuck in the error state
					// The application should close the connection
					self.stm = ConnSTM::Error(err);
					break Err(err);
				},
			}
		};

		// If we read any data drain it out of the rxbuffer
		// TODO: Make this more efficient
		if offset > 0 {
			rxbuffer.consume(offset);
		}

		// Carefully place the rxbuffer back in its place
		self.rxbuffer.store(rxbuffer);

		result
	}

	#[cfg(feature = "std")]
	pub fn send(&mut self, dest: &mut dyn io::Write) -> io::Result<()> {
		if self.inner.txbuffer.len() == 0 {
			return Ok(());
		}
		loop {
			match dest.write(self.inner.txbuffer.getx()) {
				Ok(n) => {
					self.inner.txbuffer.consume(n);
					return Ok(());
				},
				Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
				Err(err) => return Err(err),
			}
		}
	}
	#[cfg(feature = "std")]
	pub fn send_all(&mut self, dest: &mut dyn io::Write) -> io::Result<()> {
		let len = self.inner.txbuffer.len();
		if len > 0 {
			// FIXME! Work with non-blocking sockets!
			// This may error with WouldBlock in which case the stream is corrupted and pending data is dropped!
			dest.write_all(self.inner.txbuffer.getx())?;
			self.inner.txbuffer.consume(len);
		}
		Ok(())
	}
	pub fn send_fn<F, E>(&mut self, mut f: F) -> result::Result<(), E> where F: FnMut(&[u8]) -> result::Result<usize, E> {
		let len =self.inner.txbuffer.len();
		if len > 0 {
			let n = f(self.inner.txbuffer.getx())?;
			self.inner.txbuffer.consume(n);
		}
		Ok(())
	}
}

impl WebSocketTx {
	/// Returns the number of pending bytes in the transmit buffer.
	#[inline]
	pub fn len(&self) -> usize {
		self.0.len()
	}

	/// Sends a text message.
	pub fn send_text(&mut self, text: &str, fin: bool) {
		let header = FrameHeader {
			fin,
			extensions: 0,
			opcode: Opcode::TEXT,
			masking_key: None,
			payload_len: text.len() as u64,
		};
		let mut buffer = [0; 16];
		let header_len = header.encode(&mut buffer).unwrap();
		self.0.append(&buffer[..header_len]);
		self.0.append(text.as_bytes());
	}

	/// Sends a binary message.
	pub fn send_binary(&mut self, data: &[u8], fin: bool) {
		let header = FrameHeader {
			fin,
			extensions: 0,
			opcode: Opcode::BINARY,
			masking_key: None,
			payload_len: data.len() as u64,
		};
		let mut buffer = [0; 16];
		let header_len = header.encode(&mut buffer).unwrap();
		self.0.append(&buffer[..header_len]);
		self.0.append(data);
	}

	/// Sends the rest of a fragmented message.
	pub fn send_more(&mut self, data: &[u8], fin: bool) {
		let header = FrameHeader {
			fin,
			extensions: 0,
			opcode: Opcode::CONTINUE,
			masking_key: None,
			payload_len: data.len() as u64,
		};
		let mut buffer = [0; 16];
		let header_len = header.encode(&mut buffer).unwrap();
		self.0.append(&buffer[..header_len]);
		self.0.append(data);
	}

	/// Sends a close message.
	///
	/// It is expected to send a close message back and closing the socket after.
	pub fn send_close(&mut self, close: u16, reason: Option<&str>) {
		let reason = reason.unwrap_or("");
		let payload_len = if close != close::CLOSE_NORMAL || reason.len() > 0 { 2 + reason.len() } else { 0 };
		let header = FrameHeader {
			fin: true,
			extensions: 0,
			opcode: Opcode::CLOSE,
			masking_key: None,
			payload_len: payload_len as u64,
		};
		let mut buffer = [0; 16];
		let header_len = header.encode(&mut buffer).unwrap();
		self.0.append(&buffer[..header_len]);
		if payload_len > 0 {
			self.0.append(&close.to_le_bytes());
			self.0.append(reason.as_bytes());
		}
	}

	/// Sends a ping message.
	///
	/// It is expected to reply with a pong message with the same payload.
	pub fn send_ping(&mut self, payload: &[u8]) {
		let header = FrameHeader {
			fin: true,
			extensions: 0,
			opcode: Opcode::PING,
			masking_key: None,
			payload_len: payload.len() as u64,
		};
		let mut buffer = [0; 16];
		let header_len = header.encode(&mut buffer).unwrap();
		self.0.append(&buffer[..header_len]);
		self.0.append(payload);
	}

	/// Sends a pong message.
	pub fn send_pong(&mut self, payload: &[u8]) {
		let header = FrameHeader {
			fin: true,
			extensions: 0,
			opcode: Opcode::PONG,
			masking_key: None,
			payload_len: payload.len() as u64,
		};
		let mut buffer = [0; 16];
		let header_len = header.encode(&mut buffer).unwrap();
		self.0.append(&buffer[..header_len]);
		self.0.append(payload);
	}
}
