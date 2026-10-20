//! Single-instance IPC over a unix socket. Framing: one connection carries one message, the
//! newline-separated absolute paths (raw OS bytes), terminated by EOF. The receiver treats the bytes
//! as hostile: size-capped, relative or empty entries dropped, read timeout.

use std::ffi::OsStr;
use std::io::{self, Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Largest message accepted from a peer.
const MAX_MESSAGE_BYTES: u64 = 1024 * 1024;
/// Most paths taken from one message.
const MAX_PATHS: usize = 10_000;
const IO_TIMEOUT: Duration = Duration::from_secs(2);

/// Encodes absolute paths as the wire message. Paths that are not absolute or that contain a newline
/// (not representable in this framing) are skipped.
pub fn encode_paths(paths: &[PathBuf]) -> Vec<u8> {
    let mut out = Vec::new();
    for p in paths {
        let bytes = p.as_os_str().as_bytes();
        if !p.is_absolute() || bytes.contains(&b'\n') {
            continue;
        }
        out.extend_from_slice(bytes);
        out.push(b'\n');
    }
    out
}

/// Decodes a wire message. Empty lines and non-absolute entries are dropped; at most
/// [`MAX_PATHS`] paths are returned.
pub fn decode_paths(bytes: &[u8]) -> Vec<PathBuf> {
    bytes
        .split(|&b| b == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| PathBuf::from(OsStr::from_bytes(line)))
        .filter(|p| p.is_absolute())
        .take(MAX_PATHS)
        .collect()
}

/// Sends `paths` to a running instance listening on `socket`.
/// `Err` means nobody is (validly) listening there.
pub fn send_paths(socket: &Path, paths: &[PathBuf]) -> io::Result<()> {
    let mut stream = UnixStream::connect(socket)?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    stream.write_all(&encode_paths(paths))?;
    stream.flush()?;
    stream.shutdown(std::net::Shutdown::Write)
}

/// Result of claiming the instance socket.
pub enum Claim {
    /// Another instance is running and accepted the paths; this process should exit.
    Forwarded,
    /// This process owns the socket.
    Primary(UnixListener),
    /// The socket could not be claimed; run without single-instance support.
    Unavailable(io::Error),
}

/// Forwards `paths` to a running instance, or (when the socket is absent/stale) binds it.
pub fn claim(socket: &Path, paths: &[PathBuf]) -> Claim {
    if send_paths(socket, paths).is_ok() {
        return Claim::Forwarded;
    }
    if let Some(dir) = socket.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        return Claim::Unavailable(e);
    }
    // Nobody answered: any file at the path is a stale leftover from a crashed instance.
    match std::fs::remove_file(socket) {
        Ok(()) => log::info!("removed stale instance socket {}", socket.display()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Claim::Unavailable(e),
    }
    match UnixListener::bind(socket) {
        Ok(l) => Claim::Primary(l),
        Err(e) => Claim::Unavailable(e),
    }
}

/// Reads one message from `stream` (bounded in size and time) and decodes it.
fn read_message(mut stream: UnixStream) -> io::Result<Vec<PathBuf>> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    let mut buf = Vec::new();
    (&mut stream)
        .take(MAX_MESSAGE_BYTES + 1)
        .read_to_end(&mut buf)?;
    if buf.len() as u64 > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "instance message too large",
        ));
    }
    Ok(decode_paths(&buf))
}

/// Accept loop: for each connection calls `on_message(paths)` (even when empty, so a bare second
/// launch can still raise the window). Runs until the process exits; each connection is handled
/// under `catch_unwind` so a bad peer or callback never kills the listener.
pub fn serve<F>(listener: UnixListener, on_message: F)
where
    F: Fn(Vec<PathBuf>),
{
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let handled = catch_unwind(AssertUnwindSafe(|| match read_message(stream) {
                    Ok(paths) => on_message(paths),
                    Err(e) => log::warn!("instance message rejected: {e}"),
                }));
                if handled.is_err() {
                    log::error!("instance message handler panicked; continuing");
                }
            }
            Err(e) => {
                log::warn!("instance socket accept failed: {e}");
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

/// Removes the socket file when dropped (normal exit, early return, unwinding).
pub struct SocketGuard(pub PathBuf);

impl Drop for SocketGuard {
    fn drop(&mut self) {
        match std::fs::remove_file(&self.0) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => log::warn!("could not remove instance socket {}: {e}", self.0.display()),
        }
    }
}

/// Raw bytes → `PathBuf` helper for tests and callers holding non-UTF-8 names.
#[cfg(test)]
fn path_from_bytes(b: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    PathBuf::from(std::ffi::OsString::from_vec(b.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn encode_decode_round_trip_including_non_utf8() {
        let paths = vec![
            PathBuf::from("/a/b c/shot_####.exr"),
            path_from_bytes(b"/odd/\xff\xfe.mov"),
            PathBuf::from("/x"),
        ];
        assert_eq!(decode_paths(&encode_paths(&paths)), paths);
    }

    #[test]
    fn encode_skips_relative_and_newline_paths() {
        let paths = vec![
            PathBuf::from("rel/x.mov"),
            PathBuf::from("/has\nnewline.mov"),
            PathBuf::from("/ok.mov"),
        ];
        assert_eq!(encode_paths(&paths), b"/ok.mov\n");
    }

    #[test]
    fn decode_drops_empty_and_relative_lines() {
        let got = decode_paths(b"\n/a\n\nrel\n../up\n/b");
        assert_eq!(got, vec![PathBuf::from("/a"), PathBuf::from("/b")]);
        assert!(decode_paths(b"").is_empty());
    }

    #[test]
    fn decode_caps_path_count() {
        let mut msg = Vec::new();
        for i in 0..(MAX_PATHS + 50) {
            msg.extend_from_slice(format!("/p{i}\n").as_bytes());
        }
        assert_eq!(decode_paths(&msg).len(), MAX_PATHS);
    }

    #[test]
    fn claim_binds_then_second_claim_forwards() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("run").join("instance.sock");

        let Claim::Primary(listener) = claim(&sock, &[]) else {
            panic!("first claim must bind");
        };
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            serve(listener, move |paths| {
                let _ = tx.send(paths);
            });
        });

        let sent = vec![
            PathBuf::from("/media/clip.mov"),
            PathBuf::from("/media/seq_####.exr"),
        ];
        assert!(matches!(claim(&sock, &sent), Claim::Forwarded));
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), sent);

        // A bare launch still produces a (empty) message.
        assert!(matches!(claim(&sock, &[]), Claim::Forwarded));
        assert!(rx.recv_timeout(Duration::from_secs(5)).unwrap().is_empty());
    }

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
    fn oversized_message_is_rejected() {
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
        assert!(read_message(stream).is_err());
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
