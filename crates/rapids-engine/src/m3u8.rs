//! HLS playlist parsing (RFC 8216) and absolute-URI resolution.
//!
//! This is deliberately a parser, not a client: it turns a playlist text
//! plus the URL it was fetched from into the structure the HLS downloader
//! walks. Anything the parser cannot represent, such as a live playlist or
//! an encrypted method it does not implement, is reported as an error so the
//! downloader can fail loudly instead of silently writing the wrong bytes.

/// The AES method HLS uses, and everything else, which the downloader must
/// refuse rather than guess at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyMethod {
    /// `#EXT-X-KEY:METHOD=AES-128`, the only encryption this engine decodes.
    Aes128,
    /// A method the engine cannot decode.
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub method: KeyMethod,
    /// Absolute URI of the key resource.
    pub uri: String,
    /// The IV from the playlist, or `None` for the segment's sequence number.
    pub iv: Option<[u8; 16]>,
}

impl Key {
    /// The IV to use when the playlist did not name one: the segment's
    /// sequence number as a 128-bit big-endian value.
    pub fn iv_for_sequence(&self, sequence: u64) -> [u8; 16] {
        self.iv
            .unwrap_or_else(|| crate::aes::iv_from_sequence(sequence))
    }
}

/// An `#EXT-X-MAP` init segment, which every audio/video segment needs in
/// front of it when the playlist carries one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitSegment {
    pub uri: String,
    pub byte_range: Option<(u64, u64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaSegment {
    pub sequence: u64,
    pub duration_seconds: f64,
    pub url: String,
    /// The key in force when this segment was listed, already resolved.
    pub key: Option<Key>,
    pub init: Option<InitSegment>,
    pub discontinuity: bool,
    pub discontinuity_sequence: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub bandwidth: u64,
    pub codecs: Option<String>,
    pub resolution: Option<(u32, u32)>,
    pub name: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaGroup {
    pub group_type: String,
    pub uri: String,
    pub name: Option<String>,
    pub language: Option<String>,
    pub default: bool,
    pub auto_select: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaPlaylist {
    pub segments: Vec<MediaSegment>,
    pub groups: Vec<MediaGroup>,
    /// `true` when the playlist carries `#EXT-X-ENDLIST`.
    pub end_list: bool,
    pub target_duration: u32,
    pub media_sequence: u64,
}

impl MediaPlaylist {
    /// A playlist without an end list is live: it has no final segment, so
    /// there is nothing to download in full.
    pub fn is_live(&self) -> bool {
        !self.end_list
    }

    /// Total listed duration in seconds.
    pub fn duration_seconds(&self) -> f64 {
        self.segments
            .iter()
            .map(|segment| segment.duration_seconds)
            .sum()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MasterPlaylist {
    pub variants: Vec<Variant>,
}

impl MasterPlaylist {
    /// The variant with the highest declared bandwidth.
    pub fn best_variant(&self) -> Option<&Variant> {
        self.variants.iter().max_by_key(|variant| variant.bandwidth)
    }
}

// Segment durations are floats, so a playlist compares but is not ordered.
#[derive(Debug, Clone, PartialEq)]
pub enum Playlist {
    Master(MasterPlaylist),
    Media(MediaPlaylist),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaylistError {
    /// The document is not an HLS playlist at all.
    NotAPlaylist,
    /// A playlist with no variants or no segments.
    Empty(String),
    /// The playlist uses an encryption method the engine cannot decode.
    UnsupportedMethod(String),
    /// A malformed line the parser could not make sense of.
    Malformed(String),
}

impl std::fmt::Display for PlaylistError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlaylistError::NotAPlaylist => {
                formatter.write_str("the document is not an HLS playlist")
            }
            PlaylistError::Empty(what) => formatter.write_str(what),
            PlaylistError::UnsupportedMethod(method) => {
                write!(formatter, "unsupported HLS encryption method {method}")
            }
            PlaylistError::Malformed(reason) => write!(formatter, "malformed playlist: {reason}"),
        }
    }
}

impl std::error::Error for PlaylistError {}

/// Resolves `reference` against `base`, both as they appear in the playlist.
/// Relative references, network-path references and absolute references are
/// all handled, and `.`/`..` segments are removed the way RFC 8216 requires.
pub fn resolve_url(base: &str, reference: &str) -> Option<String> {
    let reference = reference.trim();
    if reference.is_empty() {
        return None;
    }
    // Strip a fragment; it is a label for a player, not part of the resource.
    let reference = match reference.find('#') {
        Some(index) => &reference[..index],
        None => reference,
    };
    if reference.is_empty() {
        return None;
    }
    if reference.contains("://") {
        return Some(reference.to_string());
    }

    let parsed = parse_url(base)?;

    if reference.starts_with("//") {
        let (authority, rest) = split_authority(&reference[2..]);
        let (path, query) = split_path_query(rest);
        return Some(join(&parsed.scheme, &authority, &path, query.as_deref()));
    }
    if reference.starts_with('/') {
        let (path, query) = split_path_query(reference);
        return Some(join(
            &parsed.scheme,
            &parsed.authority,
            &path,
            query.as_deref(),
        ));
    }

    let (reference_path, reference_query) = split_path_query(reference);
    let merged = merge_path(&parsed.path, &reference_path);
    Some(join(
        &parsed.scheme,
        &parsed.authority,
        &merged,
        reference_query.as_deref(),
    ))
}

struct ParsedUrl {
    scheme: String,
    authority: String,
    path: String,
}

fn parse_url(url: &str) -> Option<ParsedUrl> {
    let (scheme, rest) = url.split_once("://")?;
    if scheme.is_empty()
        || !scheme.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
        })
    {
        return None;
    }
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, ""),
    };
    Some(ParsedUrl {
        scheme: scheme.to_ascii_lowercase(),
        authority: authority.to_string(),
        path: path.to_string(),
    })
}

/// Splits `host[:port][/path...]` at the first path separator.
fn split_authority(text: &str) -> (String, &str) {
    let without_userinfo = match text.rfind('@') {
        Some(index) => &text[index + 1..],
        None => text,
    };
    match without_userinfo.find('/') {
        Some(index) => (
            without_userinfo[..index].to_string(),
            &without_userinfo[index..],
        ),
        None => (without_userinfo.to_string(), ""),
    }
}

fn split_path_query(text: &str) -> (String, Option<String>) {
    match text.find('?') {
        Some(index) => (
            text[..index].to_string(),
            Some(text[index + 1..].to_string()),
        ),
        None => (text.to_string(), None),
    }
}

fn join(scheme: &str, authority: &str, path: &str, query: Option<&str>) -> String {
    let mut out = format!("{scheme}://{authority}");
    if !path.is_empty() && !path.starts_with('/') {
        out.push('/');
    }
    out.push_str(path);
    if let Some(query) = query
        && !query.is_empty()
    {
        out.push('?');
        out.push_str(query);
    }
    out
}

/// Joins a relative reference onto the directory part of a base path.
fn merge_path(base_path: &str, reference_path: &str) -> String {
    let base_directory = match base_path.rfind('/') {
        Some(index) => &base_path[..index + 1],
        None => "",
    };
    remove_dot_segments(&format!("{base_directory}{reference_path}"))
}

/// RFC 3986 section 5.2.4: resolve `.` and `..` against the path.
fn remove_dot_segments(input: &str) -> String {
    let absolute = input.starts_with('/');
    let mut kept: Vec<&str> = Vec::new();
    for segment in input.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                kept.pop();
            }
            other => kept.push(other),
        }
    }
    let joined = kept.join("/");
    if absolute && !joined.starts_with('/') {
        format!("/{joined}")
    } else {
        joined
    }
}

/// Splits an attribute list into lower-cased keys and values, honouring
/// quoted values that may contain commas.
pub fn parse_attributes(text: &str) -> Vec<(String, String)> {
    let mut attributes = Vec::new();
    let mut rest = text;
    loop {
        rest = rest.trim_start();
        rest = rest.strip_prefix(',').unwrap_or(rest);
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let Some(equals) = rest.find('=') else {
            break;
        };
        let key = rest[..equals].trim().to_ascii_lowercase();
        let value_start = equals + 1;
        let value = rest[value_start..].trim_start();
        let (value, consumed) = if let Some(quoted) = value.strip_prefix('"') {
            match quoted.find('"') {
                Some(end) => (quoted[..end].to_string(), end + 2),
                None => (quoted.to_string(), quoted.len()),
            }
        } else {
            let end = value
                .find(|character: char| character == ',' || character.is_whitespace())
                .unwrap_or(value.len());
            (value[..end].to_string(), end)
        };
        attributes.push((key, value));
        rest = &rest[value_start + consumed..];
    }
    attributes
}

fn attribute<'a>(attributes: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

/// Parses `IV=0x00000000000000000000000000000001`, which may be shorter
/// than 128 bits and is then left-padded with zeros.
fn parse_iv(text: &str) -> Option<[u8; 16]> {
    let digits = text.trim_start().strip_prefix("0x")?;
    if digits.len() > 32 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut padded = digits.to_string();
    if padded.len() % 2 == 1 {
        padded.insert(0, '0');
    }
    let mut value = [0u8; 16];
    let offset = 16 - padded.len() / 2;
    for index in (0..padded.len()).step_by(2) {
        value[offset + index / 2] = u8::from_str_radix(&padded[index..index + 2], 16).ok()?;
    }
    Some(value)
}

fn parse_resolution(text: &str) -> Option<(u32, u32)> {
    let (width, height) = text.split_once('x')?;
    let width = width.trim().parse::<u32>().ok()?;
    let height = height.trim().parse::<u32>().ok()?;
    Some((width, height))
}

/// Parses a playlist. The first `#EXT-X-STREAM-INF` decides whether the
/// document is a master playlist or a media playlist.
pub fn parse(text: &str, base_url: &str) -> Result<Playlist, PlaylistError> {
    if !text.lines().any(|line| line.trim() == "#EXTM3U") {
        return Err(PlaylistError::NotAPlaylist);
    }

    let lines: Vec<&str> = text.lines().collect();
    let saw_stream_inf = lines
        .iter()
        .any(|line| line.trim().strip_prefix("#EXT-X-STREAM-INF:").is_some());

    if saw_stream_inf {
        return parse_master(&lines, base_url);
    }
    parse_media(&lines, base_url)
}

fn parse_master(lines: &[&str], base_url: &str) -> Result<Playlist, PlaylistError> {
    let mut variants: Vec<Variant> = Vec::new();
    let mut pending: Option<Variant> = None;

    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            let attributes = parse_attributes(rest);
            pending = Some(Variant {
                bandwidth: attribute(&attributes, "bandwidth")
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(0),
                codecs: attribute(&attributes, "codecs").map(str::to_string),
                resolution: attribute(&attributes, "resolution").and_then(parse_resolution),
                name: attribute(&attributes, "name").map(str::to_string),
                url: String::new(),
            });
            continue;
        }
        if pending.is_some() && !line.starts_with('#') {
            let variant = pending.take().expect("checked");
            let url = resolve_url(base_url, line).ok_or_else(|| {
                PlaylistError::Malformed(format!("cannot resolve variant URL {line}"))
            })?;
            variants.push(Variant { url, ..variant });
        }
    }

    if variants.is_empty() {
        return Err(PlaylistError::Empty(
            "master playlist has no variants".to_string(),
        ));
    }
    Ok(Playlist::Master(MasterPlaylist { variants }))
}

fn parse_media(lines: &[&str], base_url: &str) -> Result<Playlist, PlaylistError> {
    let mut segments: Vec<MediaSegment> = Vec::new();
    let mut groups: Vec<MediaGroup> = Vec::new();
    let mut key: Option<Key> = None;
    let mut init: Option<InitSegment> = None;
    let mut discontinuity = false;
    let mut discontinuity_sequence = None;
    let mut media_sequence = 0u64;
    let mut target_duration = 0u32;
    let mut end_list = false;
    let mut pending_duration: Option<f64> = None;

    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXT-X-MEDIA:") {
            let attributes = parse_attributes(rest);
            let uri = attribute(&attributes, "uri")
                .ok_or_else(|| PlaylistError::Malformed("media tag without a URI".to_string()))?;
            let resolved = resolve_url(base_url, uri).ok_or_else(|| {
                PlaylistError::Malformed(format!("cannot resolve media URI {uri}"))
            })?;
            groups.push(MediaGroup {
                group_type: attribute(&attributes, "type").unwrap_or("").to_string(),
                uri: resolved,
                name: attribute(&attributes, "name").map(str::to_string),
                language: attribute(&attributes, "language").map(str::to_string),
                default: attribute(&attributes, "default")
                    .is_some_and(|value| !matches!(value, "NO" | "0" | "false")),
                auto_select: attribute(&attributes, "autoselect")
                    .is_some_and(|value| !matches!(value, "NO" | "0" | "false")),
            });
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXT-X-KEY:") {
            let attributes = parse_attributes(rest);
            let method = attribute(&attributes, "method").unwrap_or("");
            key = match method {
                "" | "NONE" => None,
                "AES-128" => {
                    let uri = attribute(&attributes, "uri").ok_or_else(|| {
                        PlaylistError::Malformed("AES-128 key without a URI".to_string())
                    })?;
                    let resolved = resolve_url(base_url, uri).ok_or_else(|| {
                        PlaylistError::Malformed(format!("cannot resolve key URI {uri}"))
                    })?;
                    Some(Key {
                        method: KeyMethod::Aes128,
                        uri: resolved,
                        iv: attribute(&attributes, "iv").and_then(parse_iv),
                    })
                }
                other => {
                    return Err(PlaylistError::UnsupportedMethod(other.to_string()));
                }
            };
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXT-X-MAP:") {
            let attributes = parse_attributes(rest);
            let uri = attribute(&attributes, "uri")
                .ok_or_else(|| PlaylistError::Malformed("EXT-X-MAP without a URI".to_string()))?;
            let resolved = resolve_url(base_url, uri).ok_or_else(|| {
                PlaylistError::Malformed(format!("cannot resolve init URI {uri}"))
            })?;
            let byte_range = attribute(&attributes, "byterange").and_then(parse_byte_range);
            init = Some(InitSegment {
                uri: resolved,
                byte_range,
            });
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let raw = rest.split(',').next().unwrap_or("").trim();
            let duration: f64 = match raw.parse() {
                Ok(value) => value,
                Err(_) => {
                    return Err(PlaylistError::Malformed(format!(
                        "bad EXTINF duration {raw}"
                    )));
                }
            };
            pending_duration = Some(duration);
            continue;
        }

        if line == "#EXT-X-DISCONTINUITY" {
            discontinuity = true;
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-DISCONTINUITY-SEQUENCE:") {
            discontinuity_sequence = rest.trim().parse::<u64>().ok();
            continue;
        }
        if line == "#EXT-X-ENDLIST" {
            end_list = true;
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-TARGETDURATION:") {
            target_duration = rest.trim().parse().unwrap_or(0);
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
            media_sequence = rest.trim().parse().unwrap_or(0);
            continue;
        }
        if line.starts_with('#') {
            // Any other tag, including ones this parser does not model, is
            // ignored: a playlist gains features over time and a downloader
            // must not break on an unknown one.
            continue;
        }

        // A bare line is a segment URL, but only directly after #EXTINF.
        let Some(duration) = pending_duration.take() else {
            continue;
        };
        let url = resolve_url(base_url, line).ok_or_else(|| {
            PlaylistError::Malformed(format!("cannot resolve segment URL {line}"))
        })?;
        let sequence = media_sequence + segments.len() as u64;
        segments.push(MediaSegment {
            sequence,
            duration_seconds: duration,
            url,
            key: key.clone(),
            init: init.take(),
            discontinuity,
            discontinuity_sequence,
        });
        discontinuity = false;
    }

    if segments.is_empty() {
        return Err(PlaylistError::Empty(
            "media playlist lists no segments".to_string(),
        ));
    }

    Ok(Playlist::Media(MediaPlaylist {
        segments,
        groups,
        end_list,
        target_duration,
        media_sequence,
    }))
}

fn parse_byte_range(text: &str) -> Option<(u64, u64)> {
    // `byterange="123-456"` as it appears in the playlist, without quotes.
    let text = text.trim();
    let (start, end) = text.split_once('-')?;
    let start = start.trim().parse::<u64>().ok()?;
    let end = end.trim().parse::<u64>().ok()?;
    Some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn media(text: &str) -> MediaPlaylist {
        match parse(text, "https://media.example/hls/playlist.m3u8").expect("parses") {
            Playlist::Media(playlist) => playlist,
            Playlist::Master(_) => panic!("expected a media playlist"),
        }
    }

    #[test]
    fn resolves_relative_references_against_the_playlist_directory() {
        assert_eq!(
            resolve_url("https://media.example/hls/master.m3u8", "segment-1.ts"),
            Some("https://media.example/hls/segment-1.ts".to_string())
        );
        assert_eq!(
            resolve_url("https://media.example/hls/master.m3u8", "/other/segment.ts"),
            Some("https://media.example/other/segment.ts".to_string())
        );
        assert_eq!(
            resolve_url(
                "https://media.example/hls/master.m3u8",
                "//cdn.example/s.ts"
            ),
            Some("https://cdn.example/s.ts".to_string())
        );
        assert_eq!(
            resolve_url(
                "https://media.example/hls/master.m3u8",
                "https://a.example/s.ts"
            ),
            Some("https://a.example/s.ts".to_string())
        );
        assert_eq!(
            resolve_url("https://media.example/hls/master.m3u8", "../up/segment.ts"),
            Some("https://media.example/up/segment.ts".to_string())
        );
        assert_eq!(
            resolve_url("https://media.example/a/b/c/master.m3u8", "../../s.ts"),
            Some("https://media.example/a/s.ts".to_string())
        );
        assert_eq!(
            resolve_url("https://media.example/hls/master.m3u8", "dir/s.ts"),
            Some("https://media.example/hls/dir/s.ts".to_string())
        );
        assert_eq!(
            resolve_url("https://media.example/hls/master.m3u8", ""),
            None
        );
    }

    #[test]
    fn a_document_without_the_m3u_marker_is_not_a_playlist() {
        assert_eq!(
            parse(
                "<!doctype html><html></html>",
                "https://media.example/x.m3u8"
            ),
            Err(PlaylistError::NotAPlaylist)
        );
    }

    #[test]
    fn a_master_playlist_keeps_its_variants_in_order() {
        let text = "#EXTM3U\n\
                    #EXT-X-STREAM-INF:BANDWIDTH=416094,RESOLUTION=640x360\n\
                    low/low.m3u8\n\
                    #EXT-X-STREAM-INF:BANDWIDTH=1110585,RESOLUTION=1280x720,CODECS=\"avc1.4d401f,mp4a.40.2\"\n\
                    high/high.m3u8\n";
        let master = match parse(text, "https://media.example/hls/master.m3u8").expect("parses") {
            Playlist::Master(master) => master,
            Playlist::Media(_) => panic!("expected a master playlist"),
        };
        assert_eq!(master.variants.len(), 2);
        assert_eq!(master.variants[0].bandwidth, 416094);
        assert_eq!(
            master.variants[0].url,
            "https://media.example/hls/low/low.m3u8"
        );
        assert_eq!(
            master.variants[1].url,
            "https://media.example/hls/high/high.m3u8"
        );
        assert_eq!(
            master.best_variant().map(|variant| variant.bandwidth),
            Some(1_110_585)
        );
        assert_eq!(
            master.variants[1].resolution,
            Some((1280, 720)),
            "resolution is parsed for the caller that wants it"
        );
        assert_eq!(
            master.variants[1].codecs.as_deref(),
            Some("avc1.4d401f,mp4a.40.2"),
            "quoted codecs keep their commas"
        );
    }

    #[test]
    fn a_media_playlist_carries_keys_and_sequences() {
        let text = "#EXTM3U\n\
                    #EXT-X-TARGETDURATION:6\n\
                    #EXT-X-MEDIA-SEQUENCE:7\n\
                    #EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\",IV=0x00000000000000000000000000000007\n\
                    #EXTINF:5.0,\n\
                    7.ts\n\
                    #EXT-X-KEY:METHOD=NONE\n\
                    #EXTINF:5.0,\n\
                    8.ts\n\
                    #EXT-X-ENDLIST\n";
        let playlist = media(text);
        assert_eq!(playlist.media_sequence, 7);
        assert_eq!(playlist.target_duration, 6);
        assert!(!playlist.is_live(), "an end list means a recording");
        assert_eq!(playlist.segments.len(), 2);
        assert_eq!(playlist.segments[0].sequence, 7);
        assert_eq!(playlist.segments[1].sequence, 8);
        assert_eq!(playlist.segments[0].url, "https://media.example/hls/7.ts");
        assert_eq!(
            playlist.segments[0].key.as_ref().map(|key| &key.uri),
            Some(&"https://media.example/hls/key.bin".to_string())
        );
        assert_eq!(
            playlist.segments[0].key.as_ref().and_then(|key| key.iv),
            Some([0u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7])
        );
        assert!(
            playlist.segments[1].key.is_none(),
            "METHOD=NONE clears the key"
        );
        assert_eq!(playlist.duration_seconds(), 10.0);
    }

    #[test]
    fn a_key_without_an_iv_uses_the_segment_sequence() {
        let text = "#EXTM3U\n\
                    #EXT-X-KEY:METHOD=AES-128,URI=key.bin\n\
                    #EXTINF:1.0,\n\
                    0001.ts\n\
                    #EXTINF:1.0,\n\
                    0002.ts\n";
        let playlist = media(text);
        assert_eq!(
            playlist.segments[1]
                .key
                .as_ref()
                .map(|key| key.iv_for_sequence(2)),
            Some([0u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2])
        );
    }

    #[test]
    fn the_init_segment_applies_to_the_segment_after_it() {
        let text = "#EXTM3U\n\
                    #EXT-X-MAP:URI=\"init.mp4\",BYTERANGE=\"154-259\"\n\
                    #EXTINF:5.0,\n\
                    1.ts\n\
                    #EXT-X-MAP:URI=\"init.mp4\"\n\
                    #EXTINF:5.0,\n\
                    2.ts\n";
        let playlist = media(text);
        assert_eq!(
            playlist.segments[0]
                .init
                .as_ref()
                .map(|init| init.uri.as_str()),
            Some("https://media.example/hls/init.mp4")
        );
        assert_eq!(
            playlist.segments[0]
                .init
                .as_ref()
                .and_then(|init| init.byte_range),
            Some((154, 259))
        );
        assert_eq!(
            playlist.segments[1]
                .init
                .as_ref()
                .and_then(|init| init.byte_range),
            None
        );
    }

    #[test]
    fn an_unknown_encryption_method_is_refused() {
        let text = "#EXTM3U\n#EXT-X-KEY:METHOD=SAMPLE-AES,URI=key.bin\n#EXTINF:1.0,\n1.ts\n";
        assert_eq!(
            parse(text, "https://media.example/hls/playlist.m3u8"),
            Err(PlaylistError::UnsupportedMethod("SAMPLE-AES".to_string()))
        );
    }

    #[test]
    fn a_playlist_without_an_end_list_is_live() {
        let text = "#EXTM3U\n#EXTINF:5.0,\n1.ts\n";
        let playlist = media(text);
        assert!(playlist.is_live());
        assert!(!playlist.end_list);
    }

    #[test]
    fn a_playlist_with_no_segments_at_all_is_empty() {
        let text = "#EXTM3U\n#EXT-X-ENDLIST\n";
        assert!(matches!(
            parse(text, "https://media.example/hls/playlist.m3u8"),
            Err(PlaylistError::Empty(_))
        ));
    }

    #[test]
    fn attributes_handle_quotes_and_missing_values() {
        let attributes = parse_attributes("BANDWIDTH=1234,CODECS=\"avc1.4d401f,mp4a.40.2\",NAME=x");
        assert_eq!(
            attributes,
            vec![
                ("bandwidth".to_string(), "1234".to_string()),
                ("codecs".to_string(), "avc1.4d401f,mp4a.40.2".to_string()),
                ("name".to_string(), "x".to_string()),
            ]
        );
    }

    #[test]
    fn a_short_iv_is_left_padded() {
        assert_eq!(
            parse_iv("0x1"),
            Some([0u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1])
        );
        assert_eq!(parse_iv("0xzz"), None);
    }

    #[test]
    fn media_groups_are_collected() {
        let text = "#EXTM3U\n\
                    #EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID=\"subs\",LANGUAGE=\"en\",URI=\"sub.m3u8\"\n\
                    #EXTINF:5.0,\n1.ts\n";
        let playlist = media(text);
        assert_eq!(playlist.groups.len(), 1);
        assert_eq!(playlist.groups[0].group_type, "SUBTITLES");
        assert_eq!(playlist.groups[0].uri, "https://media.example/hls/sub.m3u8");
        assert_eq!(playlist.groups[0].language.as_deref(), Some("en"));
    }
}
