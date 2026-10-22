use rapids_test_support::media_page::MediaPageServer;

fn main() {
    let server = MediaPageServer::start();
    println!("Page: {}", server.url("/page.html"));
    println!("Empty page: {}", server.url("/empty.html"));
    println!("Title-spoof page: {}", server.url("/spoof.html"));
    println!("Direct media: {}", server.url("/clip.mp4"));
    println!("Link-only media: {}", server.url("/trailer.mp4"));
    println!("Media SHA-256: {}", server.expected_sha256());
    println!(
        "Engine-cookie assertion (204 means at least one cookie-free engine GET): {}",
        server.url("/assert-no-engine-cookie")
    );
    loop {
        std::thread::park();
    }
}
