use core::{fmt, str};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum HttpError {
	WouldBlock,
	Invalid,
}

#[derive(Clone, Debug, Default)]
pub struct HttpHandshake {
	upgrade: bool,                   // Upgrade: websocket
	connection: bool,                // Connection: Upgrade
	websocket_version: bool,         // Sec-WebSocket-Version: 13
	websocket_key: Option<[u8; 28]>, // Sec-WebSocket-Key: encoded
}
impl HttpHandshake {
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
	pub fn is_success(&self) -> bool {
		self.upgrade && self.connection && self.websocket_version && self.websocket_key.is_some()
	}
	pub fn response<F: FnMut(&fmt::Display)>(&self, mut f: F) {
		let websocket_accept = self.websocket_key.as_ref().map(|key| &key[..]).unwrap_or(&[]);
		let websocket_accept = unsafe { str::from_utf8_unchecked(websocket_accept) };
		f(&format_args!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n", websocket_accept))
	}
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

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct HttpRequest<'a> {
	method: &'a [u8],
	resource: &'a [u8],
	version: &'a [u8],
}
fn decode_http_request<'a>(stream: &'a [u8], protocol: &mut HttpRequest<'a>) -> Result<usize, HttpError> {
	// Find the first space to delimit the method
	let mut i = 0;
	loop {
		if i >= stream.len() {
			return Err(HttpError::WouldBlock);
		}
		i += 1;
		if stream[i - 1] == b' ' {
			break;
		}
	}
	protocol.method = &stream[..i - 1];
	// Find the final \r\n and last space
	let mut j = i;
	let mut s = i;
	loop {
		if j >= stream.len() {
			return Err(HttpError::WouldBlock);
		}
		j += 1;
		if stream[j - 1] == b' ' {
			s = j;
		}
		if stream[j - 2] == b'\r' && stream[j - 1] == b'\n' {
			break;
		}
	}
	if j == s {
		return Err(HttpError::Invalid);
	}
	protocol.resource = &stream[i..s];
	protocol.version = &stream[s..j - 2];
	Ok(j)
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct HttpHeader<'a> {
	pub key: &'a [u8],
	pub value: &'a [u8],
}
pub fn decode_http_header<'a>(stream: &'a [u8], header: &mut HttpHeader<'a>) -> Result<usize, HttpError> {
	if stream.len() < 4 {
		return Err(HttpError::WouldBlock);
	}
	let mut i = 1;
	loop {
		if i >= stream.len() {
			return Err(HttpError::Invalid);
		}
		i += 1;
		if stream[i - 2] == b':' && stream[i - 1] == b' ' {
			break;
		}
	}
	header.key = &stream[..i - 2];
	let mut j = i;
	loop {
		if j >= stream.len() {
			return Err(HttpError::WouldBlock);
		}
		j += 1;
		if stream[j - 2] == b'\r' && stream[j - 1] == b'\n' {
			break;
		}
	}
	header.value = &stream[i..j - 2];
	Ok(j)
}

pub fn decode_http_handshake(stream: &[u8], http_handshake: &mut HttpHandshake) -> Result<usize, HttpError> {
	let mut request = HttpRequest::default();
	let mut header = HttpHeader::default();
	let mut i = decode_http_request(stream, &mut request)?;
	loop {
		match decode_http_header(&stream[i..], &mut header) {
			Ok(len) => {
				i += len;
				if let (Ok(key), Ok(value)) = (str::from_utf8(header.key), str::from_utf8(header.value)) {
					http_handshake.header(key, value);
				}
			},
			Err(_) => {
				if stream.len() >= i + 2 {
					return if stream[i] == b'\r' && stream[i + 1] == b'\n' { Ok(i + 2) }
					else { Err(HttpError::Invalid) };
				}
				else {
					return Err(HttpError::WouldBlock);
				}
			},
		}
	}
}

#[test]
fn test_decode_http_handshake() {

const HTTP_UPGRADE: &str = "GET /?encoding=text HTTP/1.1\r\n\
Host: 127.0.0.1:30145\r\n\
User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:65.0) Gecko/20100101 Firefox/65.0\r\n\
Accept: text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8\r\n\
Accept-Language: en-GB,en;q=0.5\r\n\
Accept-Encoding: gzip, deflate\r\n\
Sec-WebSocket-Version: 13\r\n\
Origin: https://www.websocket.org\r\n\
Sec-WebSocket-Extensions: permessage-deflate\r\n\
Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
DNT: 1\r\n\
Connection: keep-alive, Upgrade\r\n\
Pragma: no-cache\r\n\
Cache-Control: no-cache\r\n\
Upgrade: websocket\r\n\
\r\n";

	let mut handshake = HttpHandshake::default();
	assert_eq!(decode_http_handshake(HTTP_UPGRADE.as_bytes(), &mut handshake), Ok(HTTP_UPGRADE.len()));
	assert_eq!(handshake.upgrade, true);
	assert_eq!(handshake.connection, true);
	assert_eq!(handshake.websocket_version, true);
	assert_eq!(handshake.websocket_key, Some(*b"s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));
}
