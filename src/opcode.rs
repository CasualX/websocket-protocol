use core::fmt;

/// WebSocket opcode.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Opcode {
	pub(crate) byte: u8
}

#[allow(non_snake_case)]
#[inline]
pub(crate) const fn Opcode(byte: u8) -> Opcode {
	Opcode { byte }
}

impl Opcode {
	/// Indicates a continuation frame of a fragmented message.
	pub const CONTINUE: Opcode = Opcode(0x0);
	/// Indicates a text data frame.
	pub const TEXT: Opcode = Opcode(0x1);
	/// Indicates a binary data frame.
	pub const BINARY: Opcode = Opcode(0x2);
	/// Indicates a close control frame.
	pub const CLOSE: Opcode = Opcode(0x8);
	/// Indicates a ping control frame.
	pub const PING: Opcode = Opcode(0x9);
	/// Indicates a pong control frame.
	pub const PONG: Opcode = Opcode(0xA);
}

impl Opcode {
	/// Tests if the opcode indicates a control frame.
	#[inline]
	pub const fn is_control(self) -> bool {
		self.byte & 0x8 != 0
	}

	/// Tests if the opcode is valid.
	#[inline]
	pub const fn is_valid(self) -> bool {
		self.byte & !0x8 < 3
	}
}

impl fmt::Debug for Opcode {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match *self {
			Opcode::CONTINUE => f.write_str("Continue"),
			Opcode::TEXT => f.write_str("Text"),
			Opcode::BINARY => f.write_str("Binary"),
			Opcode::CLOSE => f.write_str("Close"),
			Opcode::PING => f.write_str("Ping"),
			Opcode::PONG => f.write_str("Pong"),
			_ => write!(f, "Opcode({})", self.byte),
		}
	}
}

impl From<u8> for Opcode {
	#[inline]
	fn from(byte: u8) -> Opcode {
		Opcode(byte)
	}
}
impl From<Opcode> for u8 {
	#[inline]
	fn from(opcode: Opcode) -> u8 {
		opcode.byte
	}
}
impl AsRef<u8> for Opcode {
	#[inline]
	fn as_ref(&self) -> &u8 {
		&self.byte
	}
}
impl AsMut<u8> for Opcode {
	#[inline]
	fn as_mut(&mut self) -> &mut u8 {
		&mut self.byte
	}
}
