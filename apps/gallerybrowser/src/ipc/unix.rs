//! Unix backend: a unix domain socket at the instance socket path.

use std::io::{self, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::time::Duration;

use super::{IO_TIMEOUT, read_bytes};

pub type Listener = UnixListener;

/// Connects to the socket at `socket` and sends one message. `Err` means nobody is (validly) there.
pub fn send(socket: &Path, message: &[u8]) -> io::Result<()> {
    let mut stream = UnixStream::connect(socket)?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    stream.write_all(message)?;
    stream.flush()?;
    stream.shutdown(std::net::Shutdown::Write)
}

/// Binds the socket, replacing a stale file left by a crashed instance (the caller found nobody
/// answering there).
pub fn bind(socket: &Path) -> io::Result<Listener> {
    if let Some(dir) = socket.parent() {
        std::fs::create_dir_all(dir)?;
    }
    match std::fs::remove_file(socket) {
        Ok(()) => log::info!("removed stale instance socket {}", socket.display()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    UnixListener::bind(socket)
}

/// Accepts connections forever, handing `handle` each connection's bounded, time-limited message.
pub fn serve<H: Fn(io::Result<Vec<u8>>)>(listener: Listener, handle: H) {
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => handle(
                stream
                    .set_read_timeout(Some(IO_TIMEOUT))
                    .and_then(|()| read_bytes(stream)),
            ),
            Err(e) => {
                log::warn!("instance socket accept failed: {e}");
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

/// Removes the socket file.
pub fn cleanup(socket: &Path) {
    match std::fs::remove_file(socket) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => log::warn!("could not remove instance socket {}: {e}", socket.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{Claim, MAX_MESSAGE_BYTES, SocketGuard, claim};

    #[test]
    fn stale_socket_file_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("instance.sock");
        // Bind and drop the listener without removing the file: a crashed instance's leftover.
        drop(UnixListener::bind(&sock).unwrap());
        assert!(sock.exists());
        assert!(matches!(claim(&sock, &[]), Claim::Primary(_)));
    }

    #[test]
    fn regular_file_at_socket_path_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("instance.sock");
        std::fs::write(&sock, b"junk").unwrap();
        assert!(matches!(claim(&sock, &[]), Claim::Primary(_)));
    }

    #[test]
    fn oversized_stream_message_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("instance.sock");
        let listener = UnixListener::bind(&sock).unwrap();
        let writer = std::thread::spawn(move || {
            let mut s = UnixStream::connect(&sock).unwrap();
            let chunk = vec![b'/'; 64 * 1024];
            // The receiver stops reading at the cap; a broken pipe on the tail is expected.
            for _ in 0..(MAX_MESSAGE_BYTES as usize / chunk.len() + 4) {
                if s.write_all(&chunk).is_err() {
                    break;
                }
            }
        });
        let (stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(IO_TIMEOUT)).unwrap();
        assert!(read_bytes(stream).is_err());
        let _ = writer.join();
    }

    #[test]
    fn guard_removes_socket() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("instance.sock");
        let l = UnixListener::bind(&sock).unwrap();
        {
            let _g = SocketGuard(sock.clone());
        }
        assert!(!sock.exists());
        drop(l);
    }
}
