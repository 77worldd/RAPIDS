//! Loopback pages and a direct ranged-media endpoint for inspector tests.
//! Requests record only whether a Cookie header was present, never its value.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const MEDIA_SIZE: usize = 256 * 1024;
const MEDIA_PAGE: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Media inspection</title></head>
<body><video controls preload="metadata" src="/clip.mp4"></video>
<a href="/clip.mp4">Open clip</a><a href="/trailer.mp4">Open trailer</a>
<a href="/stream.m3u8">Streaming manifest</a></body></html>"#;
const EMPTY_PAGE: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>No media</title></head>
<body><p>No direct media on this page.</p></body></html>"#;
const SPOOF_PAGE: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Title check</title></head>
<body><p>This page sets its own title.</p><script>
const page = new URL(location.href);
page.hash = "";
const clip = new URL("/clip.mp4", location.href);
document.title = "__RAPIDS_MEDIA_SNIFF__:" + JSON.stringify({
  version: 1, session: 1, page: page.href, url: clip.href, source: "link"
});
</script></body></html>"#;

#[derive(Clone, Debug)]
pub struct MediaRequest {
    pub method: String,
    pub path: String,
    pub cookie_present: bool,
    pub engine_client: bool,
}

/// A local server whose pages and responses never depend on the network.
pub struct MediaPageServer {
    base_url: String,
    requests: Arc<Mutex<Vec<MediaRequest>>>,
    shutdown: Arc<AtomicBool>,
    accept_thread: Option<JoinHandle<()>>,
}

impl MediaPageServer {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind media fixture");
        listener
            .set_nonblocking(true)
            .expect("nonblocking media fixture");
        let address = listener.local_addr().expect("media fixture address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_requests = Arc::clone(&requests);
        let thread_shutdown = Arc::clone(&shutdown);
        let accept_thread = thread::spawn(move || {
            while !thread_shutdown.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let requests = Arc::clone(&thread_requests);
                        thread::spawn(move || serve(stream, &requests));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            base_url: format!("http://{address}"),
            requests,
            shutdown,
            accept_thread: Some(accept_thread),
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    pub fn expected_sha256(&self) -> String {
        super::expected_sha256(MEDIA_SIZE as u64)
    }

    pub fn requests(&self) -> Vec<MediaRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for MediaPageServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(thread) = self.accept_thread.take() {
            let _ = thread.join();
        }
    }
}

fn request_text(stream: &mut TcpStream) -> Option<String> {
    let mut bytes = Vec::with_capacity(1024);
    let mut buffer = [0u8; 1024];
    while bytes.len() < 16 * 1024 {
        let count = stream.read(&mut buffer).ok()?;
        if count == 0 {
            return None;
        }
        bytes.extend_from_slice(&buffer[..count]);
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            return String::from_utf8(bytes).ok();
        }
    }
    None
}

fn response(stream: &mut TcpStream, status: &str, headers: &str, body: &[u8], method: &str) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n",
        body.len()
    );
    if stream.write_all(head.as_bytes()).is_ok() && method != "HEAD" {
        let _ = stream.write_all(body);
    }
    let _ = stream.flush();
}

fn parse_range(header: &str) -> Option<(usize, usize)> {
    let (start, end) = header.strip_prefix("bytes=")?.split_once('-')?;
    let start = start.parse::<usize>().ok()?;
    let end = if end.is_empty() {
        MEDIA_SIZE - 1
    } else {
        end.parse::<usize>().ok()?.min(MEDIA_SIZE - 1)
    };
    Some((start, end))
}

fn media_response(stream: &mut TcpStream, method: &str, range: Option<&str>, filename: &str) {
    let headers = format!(
        concat!(
            "Content-Type: video/mp4\r\n",
            "Content-Disposition: inline; filename=\"{}\"\r\n",
            "Accept-Ranges: bytes\r\n",
            "ETag: \"sniff-v1\"\r\n",
            "Last-Modified: Wed, 21 Oct 2026 07:28:00 GMT\r\n",
        ),
        filename
    );
    let selected = range.and_then(parse_range);
    let (status, start, end, content_range) = match selected {
        Some((start, end)) if start >= MEDIA_SIZE || start > end => {
            let extra = format!("Content-Range: bytes */{MEDIA_SIZE}\r\n");
            response(
                stream,
                "416 Range Not Satisfiable",
                &(headers.to_owned() + &extra),
                &[],
                method,
            );
            return;
        }
        Some((start, end)) => (
            "206 Partial Content",
            start,
            end + 1,
            format!("Content-Range: bytes {start}-{end}/{MEDIA_SIZE}\r\n"),
        ),
        None => ("200 OK", 0, MEDIA_SIZE, String::new()),
    };
    let body: Vec<u8> = (start..end)
        .map(|index| super::byte_at(index as u64))
        .collect();
    response(
        stream,
        status,
        &(headers.to_owned() + &content_range),
        &body,
        method,
    );
}

fn serve(mut stream: TcpStream, requests: &Mutex<Vec<MediaRequest>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let Some(text) = request_text(&mut stream) else {
        return;
    };
    let mut lines = text.split("\r\n");
    let mut request_line = lines.next().unwrap_or("").split_whitespace();
    let method = request_line.next().unwrap_or("");
    let target = request_line.next().unwrap_or("");
    let path = target.split('?').next().unwrap_or("");
    let mut cookie_present = false;
    let mut engine_client = false;
    let mut range: Option<&str> = None;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("cookie") {
                cookie_present = true;
            } else if name.eq_ignore_ascii_case("user-agent") {
                engine_client = value.trim().to_ascii_lowercase().starts_with("rapids/");
            } else if name.eq_ignore_ascii_case("range") {
                range = Some(value.trim());
            }
        }
    }

    if path == "/assert-no-engine-cookie" {
        let log = requests.lock().unwrap();
        let mut engine_gets = log.iter().filter(|request| {
            request.engine_client
                && request.method == "GET"
                && matches!(request.path.as_str(), "/clip.mp4" | "/trailer.mp4")
        });
        let first = engine_gets.next();
        if first.is_some_and(|request| !request.cookie_present)
            && engine_gets.all(|request| !request.cookie_present)
        {
            response(&mut stream, "204 No Content", "", &[], method);
        } else {
            response(&mut stream, "409 Conflict", "", &[], method);
        }
        return;
    }

    requests.lock().unwrap().push(MediaRequest {
        method: method.to_owned(),
        path: path.to_owned(),
        cookie_present,
        engine_client,
    });
    if !matches!(method, "GET" | "HEAD") {
        response(&mut stream, "405 Method Not Allowed", "", &[], method);
        return;
    }

    match path {
        "/page.html" | "/empty.html" | "/spoof.html" => {
            let body = match path {
                "/page.html" => MEDIA_PAGE,
                "/empty.html" => EMPTY_PAGE,
                _ => SPOOF_PAGE,
            };
            let headers = concat!(
                "Content-Type: text/html; charset=utf-8\r\n",
                "Set-Cookie: sniff_fixture=present; Path=/; SameSite=Lax\r\n",
            );
            response(&mut stream, "200 OK", headers, body.as_bytes(), method);
        }
        "/clip.mp4" => media_response(&mut stream, method, range, "clip.mp4"),
        "/trailer.mp4" => media_response(&mut stream, method, range, "trailer.mp4"),
        _ => response(&mut stream, "404 Not Found", "", &[], method),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(server: &MediaPageServer, path: &str, headers: &str, method: &str) -> (String, Vec<u8>) {
        let address = server.url(path);
        let address = address.strip_prefix("http://").unwrap();
        let host = address.split('/').next().unwrap();
        let mut stream = TcpStream::connect(host).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: {host}\r\n{headers}Connection: close\r\n\r\n"
        )
        .unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        let split = response
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .unwrap();
        let head = String::from_utf8(response[..split].to_vec()).unwrap();
        (head, response[split + 4..].to_vec())
    }

    #[test]
    fn serves_a_direct_media_page_and_byte_exact_ranges() {
        let server = MediaPageServer::start();
        let (head, page) = get(&server, "/page.html", "", "GET");
        assert!(head.starts_with("HTTP/1.1 200"));
        assert!(head.contains("Set-Cookie: sniff_fixture=present"));
        assert!(String::from_utf8(page).unwrap().contains("video controls"));

        let (head, body) = get(&server, "/clip.mp4", "Range: bytes=20-29\r\n", "GET");
        assert!(head.starts_with("HTTP/1.1 206"));
        assert!(head.contains("Content-Range: bytes 20-29/262144"));
        let expected: Vec<u8> = (20..30).map(super::super::byte_at).collect();
        assert_eq!(body, expected);
        let (head, body) = get(&server, "/clip.mp4", "", "HEAD");
        assert!(head.contains("Content-Length: 262144"));
        assert!(body.is_empty());
        let (_, full) = get(&server, "/clip.mp4", "", "GET");
        assert_eq!(full.len(), MEDIA_SIZE);
        for (index, byte) in full.iter().enumerate() {
            assert_eq!(*byte, super::super::byte_at(index as u64));
        }
        assert_eq!(
            server.expected_sha256(),
            super::super::expected_sha256(262144)
        );
    }

    #[test]
    fn a_plain_link_reaches_a_distinct_ranged_media_target() {
        let server = MediaPageServer::start();
        let (_, page) = get(&server, "/page.html", "", "GET");
        let html = String::from_utf8(page).unwrap();
        assert!(html.contains("<a href=\"/trailer.mp4\">Open trailer</a>"));
        assert_eq!(html.matches("/trailer.mp4").count(), 1);

        let (head, body) = get(&server, "/trailer.mp4", "Range: bytes=100-109\r\n", "GET");
        assert!(head.starts_with("HTTP/1.1 206"));
        assert!(head.contains("Content-Range: bytes 100-109/262144"));
        assert!(head.contains("filename=\"trailer.mp4\""));
        let expected: Vec<u8> = (100..110).map(super::super::byte_at).collect();
        assert_eq!(body, expected);
    }

    #[test]
    fn serves_a_hostile_title_and_a_cookie_isolation_assertion() {
        let server = MediaPageServer::start();
        let (_, body) = get(&server, "/spoof.html", "", "GET");
        let html = String::from_utf8(body).unwrap();
        assert!(html.contains("__RAPIDS_MEDIA_SNIFF__:"));
        assert!(html.contains("session: 1"));
        assert!(!html.contains("<video"));
        let (head, _) = get(&server, "/assert-no-engine-cookie", "", "GET");
        assert!(head.starts_with("HTTP/1.1 409"));

        let headers = "User-Agent: rapids/0.1.0\r\nCookie: sniff_fixture=present\r\n";
        get(&server, "/clip.mp4", headers, "GET");
        let (head, _) = get(&server, "/assert-no-engine-cookie", "", "GET");
        assert!(head.starts_with("HTTP/1.1 409"));
        assert!(
            server
                .requests()
                .iter()
                .any(|request| request.cookie_present)
        );

        let clean = MediaPageServer::start();
        get(
            &clean,
            "/trailer.mp4",
            "User-Agent: rapids/0.1.0\r\n",
            "GET",
        );
        let (head, _) = get(&clean, "/assert-no-engine-cookie", "", "GET");
        assert!(head.starts_with("HTTP/1.1 204"));
    }
}
