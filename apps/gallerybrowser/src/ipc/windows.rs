//! Windows backend: a named pipe (`\\.\pipe\…`) whose name is derived from the instance socket path,
//! via the `interprocess` crate's local sockets. Named pipes have no read timeouts, so every
//! connection is read on its own short-lived thread; a peer that stalls ties up only that thread and
//! the number of concurrent readers is capped.

use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use interprocess::local_socket::{
    GenericNamespaced, ListenerOptions, Name, Stream, ToNsName, prelude::*,
};

use super::{IO_TIMEOUT, read_bytes};

pub type Listener = interprocess::local_socket::Listener;

/// Most connections read at the same time; further ones are dropped.
const MAX_CONCURRENT_READERS: usize = 8;

/// The pipe name for an instance socket path: stable across processes and path-safe (FNV-1a of the
/// case-folded path, as Windows paths are case-insensitive).
fn pipe_name(socket: &Path) -> io::Result<Name<'static>> {
    let folded = socket.to_string_lossy().to_lowercase();
    let hash = folded.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("io.github.nickpittas.GalleryBrowser.{hash:016x}").to_ns_name::<GenericNamespaced>()
}

/// Connects to the pipe for `socket` and sends one message, giving up after [`IO_TIMEOUT`].
/// `Err` means nobody is (validly) listening.
pub fn send(socket: &Path, message: &[u8]) -> io::Result<()> {
    let name = pipe_name(socket)?;
    let message = message.to_vec();
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("gb-instance-send".into())
        .spawn(move || {
            // No flush: it would block until the peer has read everything. The handle is closed when
            // the stream drops; bytes already written stay readable for the server.
            let result = Stream::connect(name).and_then(|mut s| s.write_all(&message));
            let _ = tx.send(result);
        })?;
    match rx.recv_timeout(IO_TIMEOUT) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "running instance did not accept the message in time",
        )),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err(io::Error::other("instance sender thread failed"))
        }
    }
}

/// Creates the pipe. Fails (address in use) while another instance owns it. The caller already
/// found nobody answering, and pipes leave no stale file behind.
pub fn bind(socket: &Path) -> io::Result<Listener> {
    ListenerOptions::new()
        .name(pipe_name(socket)?)
        .create_sync()
}

/// Counts a reader thread for the whole of its life, panics included.
struct ReaderSlot(Arc<AtomicUsize>);

impl Drop for ReaderSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn accept_loop(
    listener: Listener,
    tx: mpsc::Sender<io::Result<Vec<u8>>>,
    active: Arc<AtomicUsize>,
) {
    loop {
        let stream = match listener.accept() {
            Ok(s) => s,
            Err(e) => {
                log::warn!("instance pipe accept failed: {e}");
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
        };
        if active.fetch_add(1, Ordering::AcqRel) >= MAX_CONCURRENT_READERS {
            active.fetch_sub(1, Ordering::AcqRel);
            log::warn!("too many simultaneous instance connections; dropping one");
            continue;
        }
        let slot = ReaderSlot(Arc::clone(&active));
        let tx = tx.clone();
        let spawned = std::thread::Builder::new()
            .name("gb-instance-read".into())
            .spawn(move || {
                let _slot = slot;
                let _ = tx.send(read_bytes(stream));
            });
        if let Err(e) = spawned {
            // The closure (and with it the slot and the stream) was dropped by the failed spawn.
            log::warn!("could not start an instance reader thread: {e}");
        }
    }
}

/// Accepts connections forever, handing `handle` each connection's bounded message on the calling
/// thread.
pub fn serve<H: Fn(io::Result<Vec<u8>>)>(listener: Listener, handle: H) {
    let (tx, rx) = mpsc::channel();
    let active = Arc::new(AtomicUsize::new(0));
    let spawned = std::thread::Builder::new()
        .name("gb-instance-accept".into())
        .spawn(move || accept_loop(listener, tx, active));
    if let Err(e) = spawned {
        log::warn!("could not start the instance accept thread: {e}");
        return;
    }
    for message in rx {
        handle(message);
    }
}

/// A named pipe disappears with its listener; there is no file to remove.
pub fn cleanup(_socket: &Path) {}
