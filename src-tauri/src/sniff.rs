//! An isolated, user-opened webview for suggesting direct media URLs. Its
//! remote document has no Tauri capability; every title is untrusted input.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::webview::{DownloadEvent, NewWindowResponse, PermissionResponse};
use tauri::{AppHandle, Emitter, Manager, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub const EVENT: &str = "rapids://sniff-candidate";
const WINDOW: &str = "media-inspector";
const TITLE_PREFIX: &str = "__RAPIDS_MEDIA_SNIFF__:";
const VERSION: u8 = 2;
const MAX_TITLE_BYTES: usize = 8 * 1024;
const MAX_URL_BYTES: usize = 2048;
const MAX_CANDIDATES: usize = 16;
const MAX_REPORTS: usize = 64;
const MAX_TEXT_CHARS: usize = 300;
const MAX_SITE_CHARS: usize = 120;
const MAX_HEIGHT: u32 = 8192;
const MAX_DURATION_SECONDS: f64 = 604_800.0;
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum Source {
    Video,
    #[serde(rename = "videoSource")]
    VideoSubTag,
    Link,
    Navigation,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
struct Meta {
    title: Option<String>,
    poster: Option<String>,
    height: Option<u32>,
    duration: Option<f64>,
    page_title: Option<String>,
    site_name: Option<String>,
    og_title: Option<String>,
    twitter_title: Option<String>,
    og_image: Option<String>,
    twitter_image: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    session: u64,
    page_url: String,
    url: String,
    source: Source,
    meta: Meta,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TitleReport {
    version: u8,
    session: u64,
    page: String,
    url: String,
    source: Source,
    #[serde(default)]
    meta: Meta,
}

struct Report {
    page: Url,
    url: Url,
    source: Source,
    meta: Meta,
}

struct Inspection {
    session: u64,
    page: Option<Url>,
    seen: HashSet<String>,
    reports: usize,
}

fn http_url(raw: &str) -> Option<Url> {
    if raw.len() > MAX_URL_BYTES || raw.trim() != raw {
        return None;
    }
    let url = Url::parse(raw).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    Some(url)
}

fn without_fragment(url: &Url) -> Url {
    let mut url = url.clone();
    url.set_fragment(None);
    url
}

fn clean_text(value: Option<String>, limit: usize) -> Option<String> {
    let text = value?;
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    Some(collapsed.chars().take(limit).collect())
}

fn clean_image(value: Option<String>) -> Option<String> {
    let raw = value?;
    let mut url = http_url(&raw)?;
    url.set_fragment(None);
    Some(url.to_string())
}

/// Page metadata is untrusted DOM input. Clamp every field before it crosses
/// the webview boundary; a bad optional field is dropped, not trusted.
fn normalize_meta(meta: Meta) -> Meta {
    Meta {
        title: clean_text(meta.title, MAX_TEXT_CHARS),
        poster: clean_image(meta.poster),
        height: meta
            .height
            .filter(|height| (1..=MAX_HEIGHT).contains(height)),
        duration: meta.duration.filter(|duration| {
            duration.is_finite() && *duration > 0.0 && *duration <= MAX_DURATION_SECONDS
        }),
        page_title: clean_text(meta.page_title, MAX_TEXT_CHARS),
        site_name: clean_text(meta.site_name, MAX_SITE_CHARS),
        og_title: clean_text(meta.og_title, MAX_TEXT_CHARS),
        twitter_title: clean_text(meta.twitter_title, MAX_TEXT_CHARS),
        og_image: clean_image(meta.og_image),
        twitter_image: clean_image(meta.twitter_image),
    }
}

fn media_path(url: &Url) -> bool {
    let path = url.path().to_ascii_lowercase();
    [".mp4", ".webm", ".m4v", ".mov", ".mkv"]
        .iter()
        .any(|extension| path.ends_with(extension))
}

fn manifest_path(url: &Url) -> bool {
    let path = url.path().to_ascii_lowercase();
    path.ends_with(".m3u8") || path.ends_with(".mpd")
}

fn parse_title(title: &str, session: u64) -> Option<Report> {
    if title.len() > MAX_TITLE_BYTES {
        return None;
    }
    let payload = title.strip_prefix(TITLE_PREFIX)?;
    let report: TitleReport = serde_json::from_str(payload).ok()?;
    if report.version != VERSION || report.session != session || report.source == Source::Navigation
    {
        return None;
    }
    let page = http_url(&report.page)?;
    let url = http_url(&report.url)?;
    if page.fragment().is_some()
        || url.fragment().is_some()
        || page.as_str() != report.page
        || url.as_str() != report.url
        || manifest_path(&url)
        || (report.source == Source::Link && !media_path(&url))
    {
        return None;
    }
    Some(Report {
        page,
        url,
        source: report.source,
        meta: normalize_meta(report.meta),
    })
}

impl Inspection {
    fn new(session: u64) -> Self {
        Self {
            session,
            page: None,
            seen: HashSet::new(),
            reports: 0,
        }
    }

    fn navigate(&mut self, url: &Url) -> Option<Candidate> {
        self.page = Some(without_fragment(url));
        self.seen.clear();
        self.reports = 0;
        if media_path(url) {
            self.record(without_fragment(url), Source::Navigation, Meta::default())
        } else {
            None
        }
    }

    fn report(&mut self, title: &str, actual: &Url) -> Option<Report> {
        if !title.starts_with(TITLE_PREFIX) || self.reports >= MAX_REPORTS {
            return None;
        }
        self.reports += 1;
        let report = parse_title(title, self.session)?;
        if self.page.as_ref()? != &report.page || without_fragment(actual) != report.page {
            return None;
        }
        Some(report)
    }

    fn accept(&mut self, report: &Report, actual: &Url, dom_verified: bool) -> Option<Candidate> {
        if !dom_verified
            || self.page.as_ref()? != &report.page
            || without_fragment(actual) != report.page
        {
            return None;
        }
        self.record(report.url.clone(), report.source, report.meta.clone())
    }

    fn record(&mut self, url: Url, source: Source, meta: Meta) -> Option<Candidate> {
        let page_url = self.page.as_ref()?.as_str().to_owned();
        if self.seen.len() >= MAX_CANDIDATES || !self.seen.insert(url.as_str().to_owned()) {
            return None;
        }
        Some(Candidate {
            session: self.session,
            page_url,
            url: url.to_string(),
            source,
            meta,
        })
    }
}

// The script is scoped to the inspector, and Windows also injects it into
// subframes. No remote page is granted a Tauri command or event permission.
fn initialization_script(session: u64) -> String {
    const SCRIPT: &str = r#"(() => {
  if (window.top !== window.self || !/^https?:$/.test(location.protocol)) return;
  const prefix = __PREFIX__;
  const session = __SESSION__;
  const seen = new Set();
  const pending = [];
  let timer = 0;
  let scanScheduled = false;
  let pageTitle = cleanText(document.title, 300);
  const direct = /\.(mp4|webm|m4v|mov|mkv)$/i;
  const manifest = /\.(m3u8|mpd)$/i;

  function deliver() {
    const report = pending.shift();
    if (!report) { timer = 0; return; }
    document.title = prefix + JSON.stringify(report);
    timer = window.setTimeout(deliver, 200);
  }

  function cleanText(value, limit) {
    if (typeof value !== "string") return null;
    const text = value.replace(/\s+/g, " ").trim();
    if (!text) return null;
    return text.length > limit ? text.slice(0, limit).trim() : text;
  }

  function absoluteHttp(value) {
    if (typeof value !== "string" || !value) return null;
    try {
      const url = new URL(value, document.baseURI);
      if (!/^https?:$/.test(url.protocol) || url.username || url.password) return null;
      url.hash = "";
      return url.href.length <= 2048 ? url.href : null;
    } catch { return null; }
  }

  function metaContent(names) {
    for (const name of names) {
      const node = document.querySelector(`meta[property="${name}"]`) ||
                   document.querySelector(`meta[name="${name}"]`);
      const content = node && node.getAttribute("content");
      if (content && content.trim()) return content;
    }
    return null;
  }

  function currentPageTitle() {
    // The delivery channel writes a formatted document title. Never let that
    // value become the page title on a later scan.
    if (!document.title.startsWith(prefix)) {
      pageTitle = cleanText(document.title, 300);
    }
    return pageTitle;
  }

  function pageMeta() {
    return {
      title: null,
      poster: null,
      height: null,
      duration: null,
      pageTitle: currentPageTitle(),
      siteName: cleanText(metaContent(["og:site_name"]), 120),
      ogTitle: cleanText(metaContent(["og:title"]), 300),
      twitterTitle: cleanText(metaContent(["twitter:title"]), 300),
      ogImage: absoluteHttp(metaContent(["og:image"])),
      twitterImage: absoluteHttp(metaContent(["twitter:image"]))
    };
  }

  function videoMeta(video) {
    const meta = pageMeta();
    meta.title = cleanText(
      video.getAttribute("title") || video.getAttribute("aria-label"), 300);
    meta.poster = absoluteHttp(
      video.getAttribute("poster") || video.poster);
    const height = Number(video.videoHeight);
    meta.height = Number.isInteger(height) && height > 0 && height <= 8192
      ? height : null;
    const duration = Number(video.duration);
    meta.duration = Number.isFinite(duration) && duration > 0 && duration <= 604800
      ? duration : null;
    return meta;
  }

  function suggest(value, source, meta) {
    if (!value || seen.size >= 16) return;
    try {
      const candidate = new URL(value, document.baseURI);
      if (!/^https?:$/.test(candidate.protocol) || candidate.username || candidate.password ||
          manifest.test(candidate.pathname) ||
          (source === "link" && !direct.test(candidate.pathname))) return;
      candidate.hash = "";
      const page = new URL(location.href);
      page.hash = "";
      if (candidate.href.length > 2048 || page.href.length > 2048 ||
          seen.has(candidate.href)) return;
      const report = { version: 2, session, page: page.href,
                       url: candidate.href, source, meta };
      const serialized = prefix + JSON.stringify(report);
      const bytes = typeof TextEncoder === "function"
        ? new TextEncoder().encode(serialized).length : serialized.length;
      if (bytes > 8192) return;
      seen.add(candidate.href);
      pending.push(report);
      if (!timer) deliver();
    } catch { /* An invalid DOM URL is not a candidate. */ }
  }

  function scan() {
    for (const video of document.querySelectorAll("video")) {
      const meta = videoMeta(video);
      suggest(video.currentSrc, "video", meta);
      suggest(video.getAttribute("src"), "video", meta);
      for (const source of video.querySelectorAll("source[src]")) {
        suggest(source.getAttribute("src"), "videoSource", meta);
      }
    }
    const linksMeta = pageMeta();
    for (const link of document.querySelectorAll("a[href]")) {
      suggest(link.getAttribute("href"), "link", linksMeta);
    }
  }

  function scheduleScan() {
    if (scanScheduled) return;
    scanScheduled = true;
    window.setTimeout(() => { scanScheduled = false; scan(); }, 150);
  }

  function ready() {
    if (!document.documentElement) return;
    const observer = new MutationObserver(scheduleScan);
    observer.observe(document.documentElement, {
      subtree: true, childList: true, attributes: true,
      attributeFilter: ["src", "href"]
    });
    document.addEventListener("loadedmetadata", scheduleScan, true);
    scan();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", ready, { once: true });
  } else {
    ready();
  }
})();"#;

    SCRIPT
        .replace("__PREFIX__", &serde_json::to_string(TITLE_PREFIX).unwrap())
        .replace("__SESSION__", &session.to_string())
}

// A title, even one with the right version and session, is only a request
// to check the DOM. A site-written title by itself cannot create a candidate.
fn verification_script(report: &Report) -> String {
    let page = serde_json::to_string(report.page.as_str()).unwrap();
    let url = serde_json::to_string(report.url.as_str()).unwrap();
    let source = serde_json::to_string(&report.source).unwrap();
    let script = r#"(() => {
  if (window.top !== window.self) return false;
  const page = new URL(location.href);
  page.hash = "";
  if (page.href !== __PAGE__) return false;
  const target = __URL__;
  const kind = __SOURCE__;
  function matches(value) {
    if (!value) return false;
    try {
      const candidate = new URL(value, document.baseURI);
      candidate.hash = "";
      return candidate.href === target;
    } catch { return false; }
  }
  if (kind === "video") {
    return [...document.querySelectorAll("video")].some(
      video => matches(video.currentSrc) || matches(video.getAttribute("src")));
  }
  if (kind === "videoSource") {
    return [...document.querySelectorAll("video source[src]")].some(
      source => matches(source.getAttribute("src")));
  }
  if (kind === "link") {
    return [...document.querySelectorAll("a[href]")].some(
      link => matches(link.getAttribute("href")));
  }
  return false;
})()"#;

    let (before_page, rest) = script.split_once("__PAGE__").unwrap();
    let (before_url, rest) = rest.split_once("__URL__").unwrap();
    let (before_source, after_source) = rest.split_once("__SOURCE__").unwrap();
    let mut output = String::with_capacity(script.len() + page.len() + url.len());
    output.push_str(before_page);
    output.push_str(&page);
    output.push_str(before_url);
    output.push_str(&url);
    output.push_str(before_source);
    output.push_str(&source);
    output.push_str(after_source);
    output
}

pub fn open(app: &AppHandle, caller: &WebviewWindow, raw_url: &str) -> Result<(), String> {
    if caller.label() != "main" {
        return Err("Only the main window can open the inspector".to_owned());
    }
    let url = http_url(raw_url).ok_or("Enter a complete HTTP or HTTPS page URL")?;
    if let Some(window) = app.get_webview_window(WINDOW) {
        window.navigate(url).map_err(|error| error.to_string())?;
        return window.set_focus().map_err(|error| error.to_string());
    }

    let session = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
    let inspection = Arc::new(Mutex::new(Inspection::new(session)));
    let navigation_state = Arc::clone(&inspection);
    let navigation_app = app.clone();
    let title_state = Arc::clone(&inspection);
    let title_app = app.clone();

    WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::External(url))
        .title("Inspect media")
        .inner_size(960.0, 720.0)
        .incognito(true)
        .initialization_script(initialization_script(session))
        .on_navigation(move |url| {
            if http_url(url.as_str()).is_none() {
                return false;
            }
            let candidate = navigation_state.lock().unwrap().navigate(url);
            if let Some(candidate) = candidate {
                let _ = navigation_app.emit_to("main", EVENT, candidate);
            }
            true
        })
        .on_document_title_changed(move |window, title| {
            let Ok(actual) = window.url() else {
                return;
            };
            let report = title_state.lock().unwrap().report(&title, &actual);
            let Some(report) = report else {
                return;
            };
            let script = verification_script(&report);
            let check_window = window.clone();
            let check_state = Arc::clone(&title_state);
            let check_app = title_app.clone();
            let _ = window.eval_with_callback(script, move |result| {
                if !matches!(serde_json::from_str::<bool>(&result), Ok(true)) {
                    return;
                }
                let Ok(actual) = check_window.url() else {
                    return;
                };
                let candidate = check_state.lock().unwrap().accept(&report, &actual, true);
                if let Some(candidate) = candidate {
                    let _ = check_app.emit_to("main", EVENT, candidate);
                }
            });
        })
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_download(|_, event| !matches!(event, DownloadEvent::Requested { .. }))
        .on_permission_request(|_, _| PermissionResponse::Deny)
        .build()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "http://127.0.0.1:53001/page.html";
    const SPOOF: &str = "http://127.0.0.1:53001/spoof.html";
    const MEDIA: &str = "http://127.0.0.1:53001/clip.mp4";
    const TRAILER: &str = "http://127.0.0.1:53001/trailer.mp4";

    fn title(session: u64, page: &str, url: &str, source: &str) -> String {
        format!(
            "{TITLE_PREFIX}{}",
            serde_json::json!({
                "version": 2,
                "session": session,
                "page": page,
                "url": url,
                "source": source,
            })
        )
    }

    #[test]
    fn link_selectors_are_valid_in_both_generated_scripts() {
        let report = Report {
            page: Url::parse(PAGE).unwrap(),
            url: Url::parse(TRAILER).unwrap(),
            source: Source::Link,
            meta: Meta::default(),
        };
        for script in [initialization_script(1), verification_script(&report)] {
            assert!(script.contains("document.querySelectorAll(\"a[href]\")"));
            assert!(!script.contains("aref]"));
        }
    }

    #[test]
    fn command_inputs_reject_unsafe_or_incomplete_urls() {
        assert!(http_url(PAGE).is_some());
        assert!(http_url("http://127.0.0.1:53001/page.html").is_some());
        for input in [
            "not-a-url",
            "file:///tmp/clip.mp4",
            "javascript:alert(1)",
            "https://user:secret@media.invalid/clip.mp4",
            "https://media.invalid/clip.mp4 ",
        ] {
            assert!(http_url(input).is_none(), "refused {input}");
        }
    }

    #[test]
    fn titles_need_exact_prefix_version_shape_and_limits() {
        let valid = title(4, PAGE, MEDIA, "video");
        assert!(parse_title(&valid, 4).is_some());
        assert!(parse_title("An ordinary page title", 4).is_none());
        assert!(parse_title(&valid.replace("\"version\":2", "\"version\":3"), 4).is_none());
        let extra = valid.replace("\"version\":2", "\"version\":2,\"extra\":true");
        assert!(parse_title(&extra, 4).is_none());
        let other_source = valid.replace("\"source\":\"video\"", "\"source\":\"other\"");
        assert!(parse_title(&other_source, 4).is_none());
        assert!(parse_title(&format!("{valid}garbage"), 4).is_none());
        assert!(parse_title(&"x".repeat(MAX_TITLE_BYTES + 1), 4).is_none());
    }

    #[test]
    fn page_metadata_is_clamped_and_invalid_fields_are_dropped() {
        let report = parse_title(
            &format!(
                "{TITLE_PREFIX}{}",
                serde_json::json!({
                    "version": 2,
                    "session": 4,
                    "page": PAGE,
                    "url": MEDIA,
                    "source": "video",
                    "meta": {
                        "title": "  Chapter   One  ",
                        "poster": "http://127.0.0.1:53001/poster.png#fragment",
                        "height": 1080,
                        "duration": 91.5,
                        "pageTitle": "The Movie (2024) - Example Site",
                        "siteName": "Example Site",
                        "ogTitle": "The Movie (2024)",
                        "twitterTitle": "The Movie (2024)",
                        "ogImage": "javascript:alert(1)",
                        "twitterImage": "http://127.0.0.1:53001/twitter.png"
                    }
                })
            ),
            4,
        )
        .unwrap();
        assert_eq!(report.meta.title.as_deref(), Some("Chapter One"));
        assert_eq!(
            report.meta.poster.as_deref(),
            Some("http://127.0.0.1:53001/poster.png")
        );
        assert_eq!(report.meta.height, Some(1080));
        assert_eq!(report.meta.duration, Some(91.5));
        assert_eq!(report.meta.og_title.as_deref(), Some("The Movie (2024)"));
        assert_eq!(report.meta.og_image, None);
        assert_eq!(
            report.meta.twitter_image.as_deref(),
            Some("http://127.0.0.1:53001/twitter.png")
        );

        let invalid = parse_title(
            &format!(
                "{TITLE_PREFIX}{}",
                serde_json::json!({
                    "version": 2,
                    "session": 4,
                    "page": PAGE,
                    "url": MEDIA,
                    "source": "video",
                    "meta": { "height": 9000, "duration": 604801.0 }
                })
            ),
            4,
        )
        .unwrap();
        assert_eq!(invalid.meta.height, None);
        assert_eq!(invalid.meta.duration, None);

        let unknown = format!(
            "{TITLE_PREFIX}{}",
            serde_json::json!({
                "version": 2,
                "session": 4,
                "page": PAGE,
                "url": MEDIA,
                "source": "video",
                "meta": { "unexpected": true }
            })
        );
        assert!(parse_title(&unknown, 4).is_none());
    }

    #[test]
    fn a_protocol_shaped_page_title_cannot_create_a_candidate() {
        let page = Url::parse(SPOOF).unwrap();
        let mut inspection = Inspection::new(1);
        inspection.navigate(&page);
        let forged = title(1, SPOOF, MEDIA, "link");
        let report = inspection.report(&forged, &page).unwrap();
        assert!(inspection.accept(&report, &page, false).is_none());
        assert!(inspection.seen.is_empty());
        assert!(
            inspection
                .report(&title(2, SPOOF, MEDIA, "link"), &page)
                .is_none()
        );
        assert!(
            inspection
                .report(&title(1, MEDIA, MEDIA, "link"), &page)
                .is_none()
        );
        for _ in inspection.reports..MAX_REPORTS {
            let _ = inspection.report(&forged, &page);
        }
        assert!(inspection.report(&forged, &page).is_none());
    }

    #[test]
    fn direct_urls_are_bounded_deduplicated_and_exclude_manifests() {
        let page = Url::parse(PAGE).unwrap();
        let media = Url::parse(MEDIA).unwrap();
        let mut inspection = Inspection::new(3);
        assert!(inspection.navigate(&page).is_none());
        let report = inspection
            .report(&title(3, PAGE, MEDIA, "video"), &page)
            .unwrap();
        assert!(inspection.accept(&report, &page, true).is_some());
        assert!(inspection.accept(&report, &page, true).is_none());
        assert!(
            inspection
                .report(&title(3, PAGE, MEDIA, "video"), &media)
                .is_none()
        );
        assert!(inspection.navigate(&media).is_some());
        assert!(inspection.navigate(&media).is_some());
        assert!(parse_title(&title(3, PAGE, "http://127.0.0.1/stream.m3u8", "video"), 3).is_none());
        assert!(parse_title(&title(3, PAGE, "http://127.0.0.1/stream.mpd", "video"), 3).is_none());
        assert!(parse_title(&title(3, PAGE, "http://127.0.0.1/clip.bin", "link"), 3).is_none());

        inspection.navigate(&page);
        for index in 0..MAX_CANDIDATES {
            let url = format!("http://127.0.0.1:53001/{index}.mp4");
            let report = inspection
                .report(&title(3, PAGE, &url, "video"), &page)
                .unwrap();
            assert!(inspection.accept(&report, &page, true).is_some());
        }
        let extra = inspection
            .report(&title(3, PAGE, MEDIA, "video"), &page)
            .unwrap();
        assert!(inspection.accept(&extra, &page, true).is_none());
    }
}
