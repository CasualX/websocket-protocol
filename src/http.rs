/*!
Questionable HTTP parsing code, just enough for handling WebSocket upgrade requests...

FIXME! Add limits on request, header sizes...
*/

use core::str;
use crate::*;

pub fn parse_request<B: BufferStrategy>(buffer: &[u8], ws: &mut WebSocket<B>, handler: &mut dyn Handler) -> Result<usize> {
	// Parse the method identifier
	let mut i = 0;
	let method = loop {
		if i >= buffer.len() {
			return Err(Error::WouldBlock);
		}
		if i >= 10 {
			return Err(Error::HttpAbort);
		}
		i += 1;
		if buffer[i - 1] == b' ' {
			break str::from_utf8(&buffer[..i - 1])?;
		}
	};

	// Find the final \r\n and last space
	let mut j = i;
	let mut s = i;
	loop {
		if j >= buffer.len() {
			return Err(Error::WouldBlock);
		}
		j += 1;
		if buffer[j - 1] == b' ' {
			s = j;
		}
		if buffer[j - 2] == b'\r' && buffer[j - 1] == b'\n' {
			break;
		}
	}
	if j == s {
		return Err(Error::BAD_REQUEST);
	}
	let uri = str::from_utf8(&buffer[i..s - 1])?;
	let version = str::from_utf8(&buffer[s..j - 2])?;

	#[cfg(feature = "debug-print")]
	std::println!("http_request({:?}, {:?}, {:?})", method, uri, version);

	if version != "HTTP/1.1" {
		return Err(Error::HTTP_VERSION_NOT_SUPPORTED);
	}

	handler.http_request(method, uri)?;

	ws.stm = ConnSTM::HttpHeader;
	Ok(j)
}

pub fn parse_response<B: BufferStrategy>(buffer: &[u8], ws: &mut WebSocket<B>, handler: &mut dyn Handler) -> Result<usize> {
	unimplemented!()
}

pub fn parse_header<B: BufferStrategy>(buffer: &[u8], ws: &mut WebSocket<B>, handler: &mut dyn Handler) -> Result<usize> {
	if buffer.len() < 2 {
		return Err(Error::WouldBlock);
	}
	if buffer.starts_with(b"\r\n") {
		if !ws.handshake.is_success() {
			return Err(Error::BAD_REQUEST)
		}
		#[cfg(feature = "debug-print")]
		std::println!("http_finish()");
		handler.http_finish()?;
		send_response(ws);
		ws.stm = ConnSTM::WebSocket(FrameSTM::Init);
		return Ok(2);
	}
	let mut i = 1;
	loop {
		if i >= buffer.len() {
			return Err(Error::WouldBlock);
		}
		i += 1;
		if buffer[i - 2] == b':' && buffer[i - 1] == b' ' {
			break;
		}
	}
	let key = str::from_utf8(&buffer[..i - 2])?;
	let mut j = i;
	loop {
		if j >= buffer.len() {
			return Err(Error::WouldBlock);
		}
		j += 1;
		if buffer[j - 2] == b'\r' && buffer[j - 1] == b'\n' {
			break;
		}
	}
	let value = str::from_utf8(&buffer[i..j - 2])?;
	ws.handshake.header(key, value);
	#[cfg(feature = "debug-print")]
	std::println!("http_header({:?}, {:?})", key, value);
	handler.http_header(key, value)?;
	Ok(j)
}

/// Writes a simple HTTP error response to the websocket.
///
/// NOTE! This must be called at most once while in the HTTP state of the websocket.
#[inline(never)]
pub fn error<B: BufferStrategy>(ws: &mut WebSocket<B>, err: Error) -> Error {
	let status = match err {
		Error::HttpAbort => return err,
		Error::HttpClientError(x) => 400 + x as i32,
		Error::HttpServerError(x) => 500 + x as i32,
		_ => 400,
	};

	let _ = write!(ws.inner, "HTTP/1.1 {} \r\n\r\n", status);
	err
}

//----------------------------------------------------------------

pub struct Handshake {
	pub upgrade: bool,                   // Upgrade: websocket
	pub connection: bool,                // Connection: Upgrade
	pub websocket_version: bool,         // Sec-WebSocket-Version: 13
	pub websocket_key: Option<[u8; 28]>, // Sec-WebSocket-Key: encoded
}
impl Handshake {
	pub const fn new() -> Handshake {
		Handshake {
			upgrade: false,
			connection: false,
			websocket_version: false,
			websocket_key: None,
		}
	}
	pub const fn is_success(&self) -> bool {
		self.upgrade && self.connection && self.websocket_version && self.websocket_key.is_some()
	}
}
impl Handshake {
	pub fn header(&mut self, key: &str, value: &str) {
		match key {
			"Upgrade" => {
				self.upgrade = value == "websocket";
			},
			"Connection" => {
				self.connection = value.split(",").any(|word| word.trim() == "Upgrade");
			},
			"Sec-WebSocket-Version" => {
				self.websocket_version = value == "13";
			},
			"Sec-WebSocket-Key" => {
				let mut sha1 = sha1::Sha1::new();
				sha1.update(value.as_bytes());
				sha1.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
				self.websocket_key = Some([0; 28]);
				websocket_base64_encode(&sha1.digest().bytes(), &mut self.websocket_key.as_mut().unwrap());
			},
			_ => (),
		};
	}
}

fn send_response<B: BufferStrategy>(ws: &mut WebSocket<B>) {
	let websocket_accept = ws.handshake.websocket_key.as_ref().map(|key| &key[..]).unwrap_or(&[]);
	let websocket_accept = unsafe { str::from_utf8_unchecked(websocket_accept) };
	let _ = write!(ws.inner, "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n", websocket_accept);
}

fn websocket_base64_encode(digest: &[u8; 20], key: &mut [u8; 28]) {
	// 11111122 22223333 33444444
	const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
	for i in 0..6 {
		let d1 = digest[i * 3 + 0];
		let d2 = digest[i * 3 + 1];
		let d3 = digest[i * 3 + 2];
		let b1 = (d1 & 0b11111100) >> 2;
		let b2 = (d1 & 0b00000011) << 4 | (d2 & 0b11110000) >> 4;
		let b3 = (d2 & 0b00001111) << 2 | (d3 & 0b11000000) >> 6;
		let b4 = d3 & 0b00111111;
		key[i * 4 + 0] = CHARSET[b1 as usize];
		key[i * 4 + 1] = CHARSET[b2 as usize];
		key[i * 4 + 2] = CHARSET[b3 as usize];
		key[i * 4 + 3] = CHARSET[b4 as usize];
	}
	let d1 = digest[18];
	let d2 = digest[19];
	let b1 = (d1 & 0b11111100) >> 2;
	let b2 = (d1 & 0b00000011) << 4 | (d2 & 0b11110000) >> 4;
	let b3 = (d2 & 0b00001111) << 2;
	key[24] = CHARSET[b1 as usize];
	key[25] = CHARSET[b2 as usize];
	key[26] = CHARSET[b3 as usize];
	key[27] = b'=';
}
