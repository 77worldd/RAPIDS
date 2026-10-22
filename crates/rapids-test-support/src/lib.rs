//! A deterministic local HTTP server for exercising the download engine.
//!
//! Everything runs on plain `std::net` threads so tests control timing
//! precisely: per-connection throttling, early disconnects, range support
//! that can be toggled, mutable validators, and a full request log.

pub mod media_page;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Byte value at `index` of the canonical test payload. The sequence is a
/// pure function, so any side can regenerate any window without storing
/// the whole file.
pub fn byte_at(index: u64) -> u8 {
    let mut value = index.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (value ^ (value >> 31)) as u8
}

/// SHA-256 of the canonical payload of `total` bytes, streamed.
pub fn expected_sha256(total: u64) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 16 * 1024];
    let mut index = 0u64;
    while index < total {
        let take = ((total - index) as usize).min(buffer.len());
        for slot in buffer.iter_mut().take(take) {
            *slot = byte_at(index);
            index += 1;
        }
        hasher.update(&buffer[..take]);
    }
    let digest = hasher.finalize();
    const HEX: [u8; 16] = *b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in digest.as_slice() {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

/// Behaviour knobs for one server instance.
#[derive(Clone)]
pub struct ServerOptions {
    pub total: u64,
    pub ranges: bool,
    pub etag: String,
    pub last_modified: String,
    pub filename: String,
    /// Per-connection throughput ceiling in bytes per second.
    pub throttle_bps: Option<u64>,
    /// Drop every Nth body connection early (1-based among bodies).
    pub flaky_every: Option<u32>,
    /// Bytes delivered before a flaky drop.
    pub flaky_after: u64,
}

/// Options for a server serving the canonical payload.
pub fn options(total: u64) -> ServerOptions {
    ServerOptions {
        total,
        ranges: true,
        etag: "\"v1\"".to_string(),
        last_modified: "Wed, 21 Oct 2026 07:28:00 GMT".to_string(),
        filename: "payload.bin".to_string(),
        throttle_bps: None,
        flaky_every: None,
        flaky_after: 0,
    }
}

/// One recorded request, for assertions about resume behaviour.
#[derive(Clone, Debug)]
pub struct RequestRecord {
    pub method: String,
    pub range: Option<String>,
    pub if_range: Option<String>,
    pub status: u16,
    /// Bytes actually written to the client (0 for HEAD).
    pub body_len: u64,
}

struct ServerState {
    options: Mutex<ServerOptions>,
    requests: Mutex<Vec<RequestRecord>>,
    body_counter: AtomicU64,
    /// Connections accepted but not yet finished: their request records may
    /// still be pushed.
    active: AtomicUsize,
}

/// A running local server. Dropping it stops the accept loop.
pub struct TestServer {
    base_url: String,
    state: Arc<ServerState>,
    shutdown: Arc<AtomicBool>,
    accept_thread: Option<JoinHandle<()>>,
}

impl TestServer {
    pub fn start(options: ServerOptions) -> TestServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        listener
            .set_nonblocking(true)
            .expect("nonblocking accept loop");
        let address = listener.local_addr().expect("server address");
        let state = Arc::new(ServerState {
            options: Mutex::new(options),
            requests: Mutex::new(Vec::new()),
            body_counter: AtomicU64::new(0),
            active: AtomicUsize::new(0),
        });
        let shutdown = Arc::new(AtomicBool::new(false));

        let thread_state = state.clone();
        let thread_shutdown = shutdown.clone();
        let accept_thread = thread::spawn(move || {
            while !thread_shutdown.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let state = thread_state.clone();
                        state.active.fetch_add(1, Ordering::AcqRel);
                        thread::spawn(move || {
                            handle_connection(stream, &state);
                            // The record is pushed by now, so a waiter may
                            // treat this connection as fully accounted for.
                            state.active.fetch_sub(1, Ordering::AcqRel);
                        });
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });

        TestServer {
            base_url: format!("http://{address}"),
            state,
            shutdown,
            accept_thread: Some(accept_thread),
        }
    }

    /// Full URL under this server for `path` (for example `/payload.bin`).
    pub fn url(&self, path: &str) -> String {
        let base = &self.base_url;
        format!("{base}{path}")
    }

    pub fn set_etag(&self, etag: &str) {
        let mut options = self.state.options.lock().unwrap();
        options.etag = etag.to_string();
    }

    pub fn requests(&self) -> Vec<RequestRecord> {
        self.state.requests.lock().unwrap().clone()
    }

    pub fn clear_requests(&self) {
        self.state.requests.lock().unwrap().clear();
    }

    /// Waits until no connection is mid-request. A client that is killed
    /// leaves its last connections finishing their writes and pushing their
    /// records, so byte accounting must wait for this before clearing the log.
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut idle_rounds = 0;
        while Instant::now() < deadline {
            // Two consecutive quiet rounds also cover a connection that was
            // still in the accept backlog on the first look.
            if self.state.active.load(Ordering::Acquire) == 0 {
                idle_rounds += 1;
                if idle_rounds >= 2 {
                    return true;
                }
            } else {
                idle_rounds = 0;
            }
            thread::sleep(Duration::from_millis(10));
        }
        false
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(thread) = self.accept_thread.take() {
            let _ = thread.join();
        }
    }
}

fn handle_connection(mut stream: TcpStream, state: &ServerState) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(15)));
    let options = state.options.lock().unwrap().clone();

    let request_text = match read_request(&mut stream) {
        Some(text) => text,
        None => return,
    };
    let mut lines = request_text.split("\r\n");
    let request_line = match lines.next() {
        Some(line) => line.to_string(),
        None => return,
    };
    let mut parts = request_line.split(' ');
    let method = parts.next().unwrap_or("").to_string();

    let mut range_header: Option<String> = None;
    let mut if_range_header: Option<String> = None;
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim().to_string();
            if key == "range" {
                range_header = Some(value);
            } else if key == "if-range" {
                if_range_header = Some(value);
            }
        }
    }

    let mut record = RequestRecord {
        method: method.clone(),
        range: range_header.clone(),
        if_range: if_range_header.clone(),
        status: 0,
        body_len: 0,
    };

    let response: Vec<u8> = match method.as_str() {
        "HEAD" => {
            record.status = 200;
            full_response(&options)
        }
        "GET" => {
            let total = options.total;
            let range = range_header.as_deref().and_then(parse_range_header);
            let validator_matches = match if_range_header.as_deref() {
                Some(value) => value == options.etag,
                None => true,
            };
            let usable_range = options.ranges && validator_matches && range.is_some();
            match (usable_range, range) {
                (true, Some((start, _))) if start >= total => {
                    record.status = 416;
                    format!(
                        "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{total}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .into_bytes()
                }
                (true, Some((start, end))) => {
                    let resolved_end = end
                        .unwrap_or(total.saturating_sub(1))
                        .min(total.saturating_sub(1));
                    let length = resolved_end - start + 1;
                    record.status = 206;
                    record.body_len = length;
                    let etag = &options.etag;
                    let last_modified = &options.last_modified;
                    let filename = &options.filename;
                    format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{resolved_end}/{total}\r\nContent-Length: {length}\r\nETag: {etag}\r\nLast-Modified: {last_modified}\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=\"{filename}\"\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n"
                    )
                    .into_bytes()
                }
                _ => {
                    record.status = 200;
                    record.body_len = total;
                    full_response(&options)
                }
            }
        }
        _ => {
            record.status = 405;
            b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec()
        }
    };

    let _ = stream.write_all(&response);
    let _ = stream.flush();

    // Body delivery with throttling and optional early disconnect.
    let wants_body = method == "GET" && record.status != 416;
    if wants_body && record.body_len > 0 {
        let body_number = state.body_counter.fetch_add(1, Ordering::AcqRel) + 1;
        let flaky = options
            .flaky_every
            .is_some_and(|every| every > 0 && body_number.is_multiple_of(u64::from(every)));
        let start = if record.status == 206 {
            parse_range_header(range_header.as_deref().unwrap_or(""))
                .map(|(start, _)| start)
                .unwrap_or(0)
        } else {
            0
        };
        let mut delivered = 0u64;
        while delivered < record.body_len {
            let remaining = record.body_len - delivered;
            let mut chunk_len = (16 * 1024u64).min(remaining);
            if flaky {
                let cap = options.flaky_after.saturating_sub(delivered);
                if cap == 0 {
                    break;
                }
                chunk_len = chunk_len.min(cap);
            }
            let mut chunk = vec![0u8; chunk_len as usize];
            for (position, slot) in chunk.iter_mut().enumerate() {
                *slot = byte_at(start + delivered + position as u64);
            }
            if let Some(rate) = options.throttle_bps {
                let rate = rate.max(1);
                let slice_ms = ((chunk_len * 1000) / rate).max(1);
                thread::sleep(Duration::from_millis(slice_ms));
            }
            if stream.write_all(&chunk).is_err() {
                break;
            }
            let _ = stream.flush();
            delivered += chunk_len;
            if flaky && delivered >= options.flaky_after {
                break;
            }
        }
    }

    state.requests.lock().unwrap().push(record);
}

/// The complete-body answer, shared by HEAD and by a GET whose range cannot
/// be honoured.
fn full_response(options: &ServerOptions) -> Vec<u8> {
    let total = options.total;
    let etag = &options.etag;
    let last_modified = &options.last_modified;
    let filename = &options.filename;
    let accept_ranges = if options.ranges {
        "\r\nAccept-Ranges: bytes"
    } else {
        ""
    };
    format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nETag: {etag}\r\nLast-Modified: {last_modified}\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=\"{filename}\"{accept_ranges}\r\nConnection: close\r\n\r\n"
    )
    .into_bytes()
}

fn read_request(stream: &mut TcpStream) -> Option<String> {
    let mut collected = Vec::with_capacity(1024);
    let mut buffer = [0u8; 1024];
    while collected.len() < 32 * 1024 {
        let read = stream.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        collected.extend_from_slice(&buffer[..read]);
        if collected.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    if collected.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&collected).into_owned())
}

fn parse_range_header(value: &str) -> Option<(u64, Option<u64>)> {
    let rest = value.trim().strip_prefix("bytes=")?;
    let (start, end) = rest.split_once('-')?;
    let start = start.trim().parse::<u64>().ok()?;
    let end = end.trim();
    if end.is_empty() {
        Some((start, None))
    } else {
        Some((start, Some(end.parse::<u64>().ok()?)))
    }
}
