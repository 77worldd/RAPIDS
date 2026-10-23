//! Receives browser handoffs through the `rapids://add` protocol.
//!
//! Parsing is deliberately pure and conservative. A valid link is queued for
//! the frontend, which opens the normal Add download confirmation sheet. The
//! host never creates a task from a browser link by itself.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_deep_link::DeepLinkExt;
use url::Url;

const ADD_EVENT: &str = "deeplink://add";
const ERROR_EVENT: &str = "deeplink://error";
const MAX_RAW_LENGTH: usize = 4096;
const MAX_NAME_LENGTH: usize = 255;
const DEDUPE_WINDOW: Duration = Duration::from_secs(30);
const DEDUPE_CAPACITY: usize = 512;
const PENDING_CAPACITY: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Video,
    Audio,
    File,
}

impl MediaKind {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "video" => Some(Self::Video),
            "audio" => Some(Self::Audio),
            "file" => Some(Self::File),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddRequest {
    pub url: Url,
    pub name: Option<String>,
    pub kind: MediaKind,
    pub source_page: Option<Url>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeepLinkError {
    UnsupportedAction,
    MissingUrl,
    BadScheme,
    TooLong,
    BadParam(String),
    Duplicate,
}

impl fmt::Display for DeepLinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedAction => {
                formatter.write_str("This RAPIDS link uses an unsupported action.")
            }
            Self::MissingUrl => {
                formatter.write_str("The browser link does not include a download URL.")
            }
            Self::BadScheme => formatter.write_str(
                "Only secure rapids:// links with HTTP or HTTPS downloads are accepted.",
            ),
            Self::TooLong => {
                formatter.write_str("The browser link is longer than 4096 characters.")
            }
            Self::BadParam(parameter) => {
                write!(
                    formatter,
                    "The browser link has an invalid {parameter} value."
                )
            }
            Self::Duplicate => {
                formatter.write_str("This download was already sent from the browser.")
            }
        }
    }
}

impl std::error::Error for DeepLinkError {}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddPayload {
    pub url: String,
    pub name: Option<String>,
    pub kind: MediaKind,
    pub source_page: Option<String>,
    pub size: Option<u64>,
}

impl From<&AddRequest> for AddPayload {
    fn from(request: &AddRequest) -> Self {
        Self {
            url: request.url.to_string(),
            name: request.name.clone(),
            kind: request.kind,
            source_page: request.source_page.as_ref().map(ToString::to_string),
            size: request.size,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PendingDeepLink {
    Add { request: AddPayload },
    Error { reason: String },
}

#[derive(Default)]
pub struct DeepLinkInbox {
    pending: Mutex<VecDeque<PendingDeepLink>>,
}

impl DeepLinkInbox {
    fn push(&self, message: PendingDeepLink) {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if pending.len() >= PENDING_CAPACITY {
            pending.pop_front();
        }
        pending.push_back(message);
    }

    fn take(&self) -> Vec<PendingDeepLink> {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        pending.drain(..).collect()
    }
}

struct Dedupe {
    seen: Mutex<HashMap<String, Instant>>,
}

impl Dedupe {
    fn new() -> Self {
        Self {
            seen: Mutex::new(HashMap::new()),
        }
    }

    /// Records a normalized URL and returns false while its 30-second window
    /// is still open. Old entries are pruned before capacity is considered.
    fn accept(&self, url: &str, now: Instant) -> bool {
        let mut seen = self
            .seen
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        seen.retain(|_, at| now.saturating_duration_since(*at) < DEDUPE_WINDOW);
        if seen.contains_key(url) {
            return false;
        }
        if seen.len() >= DEDUPE_CAPACITY
            && let Some(oldest) = seen
                .iter()
                .min_by_key(|(_, at)| **at)
                .map(|(key, _)| key.clone())
        {
            seen.remove(&oldest);
        }
        seen.insert(url.to_string(), now);
        true
    }
}

fn global_dedupe() -> &'static Dedupe {
    static GLOBAL: OnceLock<Dedupe> = OnceLock::new();
    GLOBAL.get_or_init(Dedupe::new)
}

/// Validate a raw deep link and apply the process-wide duplicate window.
pub fn parse_add_url(raw: &str) -> Result<AddRequest, DeepLinkError> {
    parse_add_url_at(raw, global_dedupe(), Instant::now())
}

fn parse_add_url_at(raw: &str, dedupe: &Dedupe, now: Instant) -> Result<AddRequest, DeepLinkError> {
    let request = validate_add_url(raw)?;
    if !dedupe.accept(request.url.as_str(), now) {
        return Err(DeepLinkError::Duplicate);
    }
    Ok(request)
}

fn validate_add_url(raw: &str) -> Result<AddRequest, DeepLinkError> {
    if raw.chars().count() > MAX_RAW_LENGTH {
        return Err(DeepLinkError::TooLong);
    }
    let link = Url::parse(raw).map_err(|_| DeepLinkError::BadScheme)?;
    if link.scheme() != "rapids" {
        return Err(DeepLinkError::BadScheme);
    }
    if link.host_str() != Some("add") {
        return Err(DeepLinkError::UnsupportedAction);
    }
    if !link.username().is_empty()
        || link.password().is_some()
        || link.port().is_some()
        || !matches!(link.path(), "" | "/")
    {
        return Err(DeepLinkError::BadScheme);
    }

    let mut url = None;
    let mut name = None;
    let mut kind = None;
    let mut page = None;
    let mut size = None;
    let mut version = None;

    for (key, value) in link.query_pairs() {
        match key.as_ref() {
            "u" if url.is_none() => url = Some(value.into_owned()),
            "n" if name.is_none() => name = Some(value.into_owned()),
            "t" if kind.is_none() => kind = Some(value.into_owned()),
            "p" if page.is_none() => page = Some(value.into_owned()),
            "sz" if size.is_none() => size = Some(value.into_owned()),
            "v" if version.is_none() => version = Some(value.into_owned()),
            _ => {}
        }
    }

    let url = url.ok_or(DeepLinkError::MissingUrl)?;
    let url = http_url(&url, "u")?;
    let kind = kind
        .as_deref()
        .map(|value| MediaKind::parse(value).ok_or_else(|| DeepLinkError::BadParam("t".into())))
        .transpose()?
        .unwrap_or(MediaKind::File);
    let source_page = page
        .as_deref()
        .map(|value| http_url(value, "p"))
        .transpose()?;
    let size = size.as_deref().map(parse_size).transpose()?;

    if let Some(value) = version {
        parse_version(&value)?;
    }

    Ok(AddRequest {
        url,
        name: name.as_deref().map(sanitize_name).transpose()?.flatten(),
        kind,
        source_page,
        size,
    })
}

fn http_url(value: &str, parameter: &str) -> Result<Url, DeepLinkError> {
    let url = Url::parse(value).map_err(|_| DeepLinkError::BadParam(parameter.into()))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(DeepLinkError::BadParam(parameter.into()));
    }
    Ok(url)
}

fn sanitize_name(value: &str) -> Result<Option<String>, DeepLinkError> {
    if value.chars().count() > MAX_NAME_LENGTH {
        return Err(DeepLinkError::BadParam("n".into()));
    }
    let cleaned: String = value
        .chars()
        .filter(|character| *character != '/' && *character != '\\' && !character.is_control())
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        Ok(None)
    } else {
        Ok(Some(cleaned.to_string()))
    }
}

fn parse_size(value: &str) -> Result<u64, DeepLinkError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(DeepLinkError::BadParam("sz".into()));
    }
    let size = value
        .parse::<u64>()
        .map_err(|_| DeepLinkError::BadParam("sz".into()))?;
    if size == 0 || size > i64::MAX as u64 {
        return Err(DeepLinkError::BadParam("sz".into()));
    }
    Ok(size)
}

fn parse_version(value: &str) -> Result<u64, DeepLinkError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(DeepLinkError::BadParam("v".into()));
    }
    let version = value
        .parse::<u64>()
        .map_err(|_| DeepLinkError::BadParam("v".into()))?;
    if version == 0 {
        return Err(DeepLinkError::BadParam("v".into()));
    }
    Ok(version)
}

/// Registers the development protocol handler and listens for both cold and
/// warm links. Warm links are forwarded by the single-instance plugin's
/// `deep-link` feature before its callback runs.
pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    if !app.manage(DeepLinkInbox::default()) {
        return Err(std::io::Error::other("the deep-link inbox is already registered").into());
    }

    #[cfg(debug_assertions)]
    app.deep_link().register_all()?;

    let handle = app.handle().clone();
    app.deep_link().on_open_url(move |event| {
        for url in event.urls() {
            handle_url(&handle, url.as_str());
        }
    });

    if let Some(urls) = app.deep_link().get_current()? {
        for url in urls {
            handle_url(app.handle(), url.as_str());
        }
    }
    Ok(())
}

fn handle_url(app: &AppHandle, raw: &str) {
    match parse_add_url(raw) {
        Ok(request) => {
            let payload = AddPayload::from(&request);
            if let Some(inbox) = app.try_state::<DeepLinkInbox>() {
                inbox.push(PendingDeepLink::Add {
                    request: payload.clone(),
                });
            }
            let _ = app.emit(ADD_EVENT, payload);
        }
        Err(DeepLinkError::Duplicate) => {}
        Err(error) => {
            let reason = error.to_string();
            if let Some(inbox) = app.try_state::<DeepLinkInbox>() {
                inbox.push(PendingDeepLink::Error {
                    reason: reason.clone(),
                });
            }
            let _ = app.emit(ERROR_EVENT, reason);
        }
    }
    focus(app);
}

/// Bring the existing window forward for both accepted and rejected links.
pub fn focus(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Draining is atomic, so the startup read and event listener cannot show the
/// same browser handoff twice.
#[tauri::command]
pub fn deeplink_take_pending(state: State<'_, DeepLinkInbox>) -> Vec<PendingDeepLink> {
    state.take()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = "rapids://add?u=https%3A%2F%2Fexample.com%2Ffile.mp4";
    const FULL: &str = concat!(
        "rapids://add?u=https%3A%2F%2Fcdn.example.com%2Fmovie.mkv",
        "&n=My%20Movie%20%E2%9C%93.mkv",
        "&t=video",
        "&p=https%3A%2F%2Fexample.com%2Fwatch%3Fx%3D1",
        "&sz=123456",
        "&v=1",
        "&unknown=ignored"
    );

    fn parse(raw: &str) -> Result<AddRequest, DeepLinkError> {
        let dedupe = Dedupe::new();
        parse_add_url_at(raw, &dedupe, Instant::now())
    }

    #[test]
    fn a_minimal_link_has_safe_defaults() {
        let request = parse(MINIMAL).expect("valid link");
        assert_eq!(request.url.as_str(), "https://example.com/file.mp4");
        assert_eq!(request.name, None);
        assert_eq!(request.kind, MediaKind::File);
        assert_eq!(request.source_page, None);
        assert_eq!(request.size, None);
    }

    #[test]
    fn all_supported_parameters_are_parsed() {
        let request = parse(FULL).expect("valid link");
        assert_eq!(request.url.as_str(), "https://cdn.example.com/movie.mkv");
        assert_eq!(request.name.as_deref(), Some("My Movie ✓.mkv"));
        assert_eq!(request.kind, MediaKind::Video);
        assert_eq!(
            request.source_page.as_ref().map(Url::as_str),
            Some("https://example.com/watch?x=1")
        );
        assert_eq!(request.size, Some(123_456));
    }

    #[test]
    fn a_missing_url_is_rejected() {
        assert_eq!(
            parse("rapids://add?n=file.mp4"),
            Err(DeepLinkError::MissingUrl)
        );
    }

    #[test]
    fn download_schemes_other_than_http_are_rejected() {
        assert_eq!(
            parse("rapids://add?u=file%3A%2F%2Fx%2Fsecret.txt"),
            Err(DeepLinkError::BadParam("u".into()))
        );
    }

    #[test]
    fn an_unknown_action_is_rejected() {
        assert_eq!(
            parse("rapids://save?u=https%3A%2F%2Fexample.com%2Ffile.mp4"),
            Err(DeepLinkError::UnsupportedAction)
        );
        assert_eq!(
            parse("https://example.com/file.mp4"),
            Err(DeepLinkError::BadScheme)
        );
    }

    #[test]
    fn an_overlong_link_is_rejected_before_parsing() {
        let raw = format!("rapids://add?u={}", "a".repeat(MAX_RAW_LENGTH));
        assert_eq!(parse(&raw), Err(DeepLinkError::TooLong));
    }

    #[test]
    fn decoded_spaces_and_unicode_survive_in_the_name() {
        let request = parse(
            "rapids://add?u=https%3A%2F%2Fexample.com%2Ffile.mp4&n=A%20caf%C3%A9%20%E2%98%95.mp4",
        )
        .expect("valid link");
        assert_eq!(request.name.as_deref(), Some("A café ☕.mp4"));
    }

    #[test]
    fn path_separators_and_controls_are_removed_from_the_name() {
        let request = parse(
            "rapids://add?u=https%3A%2F%2Fexample.com%2Ffile.mp4&n=..%2F..%5Cevil%0Aclip.mp4",
        )
        .expect("valid link");
        assert_eq!(request.name.as_deref(), Some("....evilclip.mp4"));
        assert!(
            !request
                .name
                .unwrap()
                .chars()
                .any(|character| matches!(character, '/' | '\\' | '\n'))
        );
    }

    #[test]
    fn a_duplicate_inside_thirty_seconds_is_ignored() {
        let dedupe = Dedupe::new();
        let start = Instant::now();
        assert!(parse_add_url_at(MINIMAL, &dedupe, start).is_ok());
        assert_eq!(
            parse_add_url_at(MINIMAL, &dedupe, start + Duration::from_secs(29)),
            Err(DeepLinkError::Duplicate)
        );
    }

    #[test]
    fn the_same_url_is_accepted_after_the_window() {
        let dedupe = Dedupe::new();
        let start = Instant::now();
        assert!(parse_add_url_at(MINIMAL, &dedupe, start).is_ok());
        assert!(parse_add_url_at(MINIMAL, &dedupe, start + Duration::from_secs(30)).is_ok());
    }

    #[test]
    fn a_size_above_the_signed_maximum_is_rejected() {
        assert_eq!(
            parse("rapids://add?u=https%3A%2F%2Fexample.com%2Ffile.mp4&sz=9223372036854775808"),
            Err(DeepLinkError::BadParam("sz".into()))
        );
        assert_eq!(
            parse("rapids://add?u=https%3A%2F%2Fexample.com%2Ffile.mp4&sz=0"),
            Err(DeepLinkError::BadParam("sz".into()))
        );
    }

    #[test]
    fn unknown_parameters_are_ignored() {
        let request = parse(concat!(
            "rapids://add?u=https%3A%2F%2Fexample.com%2Ffile.mp4",
            "&utm_source=browser&future=value"
        ))
        .expect("valid link");
        assert_eq!(request.url.as_str(), "https://example.com/file.mp4");
    }
}
