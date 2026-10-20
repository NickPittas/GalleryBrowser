//! Single-instance IPC over a local socket (unix domain socket at the given path; a named pipe derived
//! from that path on Windows). Framing: one connection carries one message, the newline-separated
//! absolute paths (raw OS bytes on unix, UTF-8 on Windows), terminated by EOF. The receiver treats the
//! bytes as hostile: size-capped, relative or empty entries dropped, read timeout.

use std::borrow::Cow;
use std::io::{self, Read};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as sys;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as sys;

/// Largest message accepted from a peer.
const MAX_MESSAGE_BYTES: u64 = 1024 * 1024;
/// Most paths taken from one message.
const MAX_PATHS: usize = 10_000;
const IO_TIMEOUT: Duration = Duration::from_secs(2);

/// The platform's accepting end of the instance socket.
pub type Listener = sys::Listener;

/// A path as wire bytes: the raw OS bytes on unix, UTF-8 (lossy) elsewhere.
#[cfg(unix)]
fn path_bytes(p: &Path) -> Cow<'_, [u8]> {
    use std::os::unix::ffi::OsStrExt;
    Cow::Borrowed(p.as_os_str().as_bytes())
}

#[cfg(not(unix))]
fn path_bytes(p: &Path) -> Cow<'_, [u8]> {
    match p.to_string_lossy() {
        Cow::Borrowed(s) => Cow::Borrowed(s.as_bytes()),
        Cow::Owned(s) => Cow::Owned(s.into_bytes()),
    }
}

#[cfg(unix)]
fn bytes_path(b: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(std::ffi::OsStr::from_bytes(b))
}

#[cfg(not(unix))]
fn bytes_path(b: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(b).into_owned())
}

/// Encodes absolute paths as the wire message. Paths that are not absolute or that contain a newline
/// (not representable in this framing) are skipped.
pub fn encode_paths(paths: &[PathBuf]) -> Vec<u8> {
    let mut out = Vec::new();
    for p in paths {
        let bytes = path_bytes(p);
        if !p.is_absolute() || bytes.contains(&b'\n') {
            continue;
        }
        out.extend_from_slice(&bytes);
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
        .map(bytes_path)
        .filter(|p| p.is_absolute())
        .take(MAX_PATHS)
        .collect()
}

/// Sends `paths` to a running instance listening on `socket`.
/// `Err` means nobody is (validly) listening there.
pub fn send_paths(socket: &Path, paths: &[PathBuf]) -> io::Result<()> {
    sys::send(socket, &encode_paths(paths))
}

/// Result of claiming the instance socket.
pub enum Claim {
    /// Another instance is running and accepted the paths; this process should exit.
    Forwarded,
    /// This process owns the socket.
    Primary(Listener),
    /// The socket could not be claimed; run without single-instance support.
    Unavailable(io::Error),
}

/// Forwards `paths` to a running instance, or (when the socket is absent/stale) binds it.
pub fn claim(socket: &Path, paths: &[PathBuf]) -> Claim {
    if send_paths(socket, paths).is_ok() {
        return Claim::Forwarded;
    }
    match sys::bind(socket) {
        Ok(l) => Claim::Primary(l),
        Err(e) => Claim::Unavailable(e),
    }
}

/// Reads one message from `stream` to EOF, bounded in size (the caller bounds the time).
fn read_bytes<R: Read>(stream: R) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    stream.take(MAX_MESSAGE_BYTES + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "instance message too large",
        ));
    }
    Ok(buf)
}

/// Accept loop: for each connection calls `on_message(paths)` (even when empty, so a bare second
/// launch can still raise the window). Runs until the process exits; each message is handled
/// under `catch_unwind` so a bad peer or callback never kills the listener.
pub fn serve<F>(listener: Listener, on_message: F)
where
    F: Fn(Vec<PathBuf>),
{
    sys::serve(listener, |message| {
        let handled = catch_unwind(AssertUnwindSafe(|| match message {
            Ok(bytes) => on_message(decode_paths(&bytes)),
            Err(e) => log::warn!("instance message rejected: {e}"),
        }));
        if handled.is_err() {
            log::error!("instance message handler panicked; continuing");
        }
    });
}

/// Releases the instance socket when dropped (normal exit, early return, unwinding): removes the
/// socket file on unix; a Windows named pipe vanishes with its listener, so there is nothing to do.
pub struct SocketGuard(pub PathBuf);

impl Drop for SocketGuard {
    fn drop(&mut self) {
        sys::cleanup(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    /// An absolute path for the current platform from a unix-style spelling.
    fn abs(p: &str) -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(format!("C:{}", p.replace('/', "\\")))
        } else {
            PathBuf::from(p)
        }
    }

    fn line(p: &Path) -> String {
        format!("{}\n", p.display())
    }

    #[test]
    fn encode_decode_round_trip() {
        let paths = vec![abs("/a/b c/shot_####.exr"), abs("/ünï/clip.mov"), abs("/x")];
        assert_eq!(decode_paths(&encode_paths(&paths)), paths);
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_paths_round_trip_on_unix() {
        use std::os::unix::ffi::OsStringExt;
        let odd = PathBuf::from(std::ffi::OsString::from_vec(b"/odd/\xff\xfe.mov".to_vec()));
        let paths = vec![PathBuf::from("/a"), odd, PathBuf::from("/x")];
        assert_eq!(decode_paths(&encode_paths(&paths)), paths);
    }

    #[test]
    fn encode_skips_relative_and_newline_paths() {
        let paths = vec![
            PathBuf::from("rel/x.mov"),
            abs("/has\nnewline.mov"),
            abs("/ok.mov"),
        ];
        assert_eq!(encode_paths(&paths), line(&abs("/ok.mov")).into_bytes());
    }

    #[test]
    fn decode_drops_empty_and_relative_lines() {
        let (a, b) = (abs("/a"), abs("/b"));
        let msg = format!("\n{}\nrel\n../up\n{}", line(&a), b.display());
        assert_eq!(decode_paths(msg.as_bytes()), vec![a, b]);
        assert!(decode_paths(b"").is_empty());
    }

    #[test]
    fn decode_caps_path_count() {
        let mut msg = Vec::new();
        for i in 0..(MAX_PATHS + 50) {
            msg.extend_from_slice(line(&abs(&format!("/p{i}"))).as_bytes());
        }
        assert_eq!(decode_paths(&msg).len(), MAX_PATHS);
    }

    #[test]
    fn oversized_message_is_rejected() {
        let big = vec![b'/'; MAX_MESSAGE_BYTES as usize + 1];
        assert!(read_bytes(big.as_slice()).is_err());
        let ok = vec![b'/'; MAX_MESSAGE_BYTES as usize];
        assert_eq!(
            read_bytes(ok.as_slice()).map(|b| b.len()).ok(),
            Some(ok.len())
        );
    }

    #[test]
    fn claim_binds_then_second_claim_forwards() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("run").join("instance.sock");

        let Claim::Primary(listener) = claim(&sock, &[]) else {
            panic!("first claim must bind");
        };
        let _guard = SocketGuard(sock.clone());
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            serve(listener, move |paths| {
                let _ = tx.send(paths);
            });
        });

        let sent = vec![abs("/media/clip.mov"), abs("/media/seq_####.exr")];
        assert!(matches!(claim(&sock, &sent), Claim::Forwarded));
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), sent);

        // A bare launch still produces a (empty) message.
        assert!(matches!(claim(&sock, &[]), Claim::Forwarded));
        assert!(rx.recv_timeout(Duration::from_secs(5)).unwrap().is_empty());
    }
}
