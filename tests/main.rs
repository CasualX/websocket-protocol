
#[test]
fn fuzz() {
	let mut server = websock::WebSocket::raw(websock::VecStrategy {
		capacity: 1000,
	});

	let mut client = websock::WebSocket::raw(websock::VecStrategy {
		capacity: 1000,
	});
}
