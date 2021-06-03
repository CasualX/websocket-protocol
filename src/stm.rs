use crate::{Error, FrameHeader, Opcode};

/// The WebSocket connection state machine.
pub enum ConnSTM {
	HttpRequest,
	HttpResponse,
	HttpHeader,
	WebSocket(FrameSTM),
	Error(Error),
}
impl ConnSTM {
	/// Initial state for HTTP server WebSockets.
	pub const SERVER: ConnSTM = ConnSTM::HttpRequest;
	/// Initial state for HTTP client WebSockets.
	pub const CLIENT: ConnSTM = ConnSTM::HttpResponse;
	/// Initial state for raw WebSockets.
	pub const RAW: ConnSTM = ConnSTM::WebSocket(FrameSTM::Init);
}

/// The WebSocket frame receiver state machine.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FrameSTM {
	/// Initial state on construction.
	Init,
	/// A completed message is available with the given opcode.
	Fin { opcode: Opcode },
	/// A fragmented message is being transmitted.
	Frag { frag_opcode: Opcode },
	/// A control frame was received in the middle of a fragmented message.
	FragCtrl { ctrl_opcode: Opcode, frag_opcode: Opcode },
	/// A fragmented message has been transmitted.
	FragFin { frag_opcode: Opcode },
	/// A frame was received which is not expected.
	Invalid,
}
impl FrameSTM {
	pub fn next(self, header: &FrameHeader) -> FrameSTM {
		match self {
			// From the Init and Fin state:
			// * Continue opcode is invalid, as there is no previous frame that started a fragmented message
			// * Fin bit indicates a completed message arrived in a single frame
			// * Otherwise a fragmented message has started, throw out any fragmented control frames
			FrameSTM::Init | FrameSTM::Fin { .. } | FrameSTM::FragFin { .. } => {
				if header.opcode == Opcode::CONTINUE {
					FrameSTM::Invalid
				}
				else if header.fin {
					FrameSTM::Fin { opcode: header.opcode }
				}
				else if header.opcode.is_control() {
					FrameSTM::Invalid
				}
				else {
					FrameSTM::Frag { frag_opcode: header.opcode }
				}
			},
			// When expecting a fragmented message:
			// * Control frames are allowed to intersperse the fragmented message, keep track of the fragmented opcode
			// * If not a control frame the opcode must be Continue
			// * Fin bit indicates the final frame, otherwise expect more fragmented frames
			FrameSTM::Frag { frag_opcode } => {
				if header.opcode.is_control() {
					if header.fin {
						FrameSTM::FragCtrl { ctrl_opcode: header.opcode, frag_opcode }
					}
					else {
						FrameSTM::Invalid
					}
				}
				else if header.opcode == Opcode::CONTINUE {
					if header.fin {
						FrameSTM::FragFin { frag_opcode }
					}
					else {
						FrameSTM::Frag { frag_opcode }
					}
				}
				else {
					FrameSTM::Invalid
				}
			},
			// Handling a control message in between fragmented frames
			FrameSTM::FragCtrl { ctrl_opcode: _, frag_opcode } => {
				if header.opcode.is_control() {
					if header.fin {
						FrameSTM::FragCtrl { ctrl_opcode: header.opcode, frag_opcode }
					}
					else {
						FrameSTM::Invalid
					}
				}
				else if header.opcode == Opcode::CONTINUE {
					if header.fin {
						FrameSTM::Fin { opcode: frag_opcode }
					}
					else {
						FrameSTM::Frag { frag_opcode }
					}
				}
				else {
					FrameSTM::Invalid
				}
			},
			FrameSTM::Invalid => {
				FrameSTM::Invalid
			},
		}
	}
}
