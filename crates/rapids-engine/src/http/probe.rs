use reqwest::header::{
    ACCEPT_RANGES, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, ETAG,
    LAST_MODIFIED, RANGE,
};
use reqwest::Response;

use crate::config::EngineConfig;
use crate::error::EngineError;

use super::{header_accepts_bytes, header_str, header_u64};

/// What the server told us about the file during the probe phase.
#[derive(Debug, Clone)]
pub(crate) struct ProbeResult {
    pub final_url: String,
    pub total: Option<u64>,
    pub accepts_ranges: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub content_type: Option<String>,
    pub filename: Option<String>,
}

/// A probe failure worth retrying versus one that ends the task.
pub(crate) enum ProbeFailure {
    Retry { reason: String, timed_out: bool },
    Fatal(EngineError),
}

/// Discovers size, range support, validators, and a filename. Retries with
/// backoff while the failure class allows it; 4xx statuses other than the
/// transient ones end the task with a readable message.
pub(crate) async fn probe(
    url: &str,
    config: &EngineConfig,
    client: &reqwest::Client,
) -> Result<ProbeResult, EngineError> {
    if let Err(error) = reqwest::Url::parse(url) {
        return Err(EngineError::InvalidUrl {
            url: url.to_string(),
            reason: error.to_string(),
        });
    }

    let mut attempt = 0u32;
    loop {
        attempt += 1;
        match probe_once(url, config, client).await {
            Ok(result) => return Ok(result),
            Err(ProbeFailure::Fatal(error)) => return Err(error),
            Err(ProbeFailure::Retry { reason, timed_out }) => {
                if attempt >= config.max_attempts {
                    return if timed_out {
                        Err(EngineError::timeout("server probe", reason, attempt))
                    } else {
                        Err(EngineError::network("server probe", reason, attempt))
                    };
                }
                let base = super::base_backoff(config, attempt);
                let delay = super::jittered_backoff(base, super::now_entropy());
                tokio::time::sleep(delay).await;
            }
        }
    }
}

async fn probe_once(
    url: &str,
    config: &EngineConfig,
    client: &reqwest::Client,
) -> Result<ProbeResult, ProbeFailure> {
    let mut result = ProbeResult {
        final_url: String::new(),
        total: None,
        accepts_ranges: false,
        etag: None,
        last_modified: None,
        content_type: None,
        filename: None,
    };

    // --- HEAD: cheap metadata when the server allows it -----------------
    let head_response = match tokio::time::timeout(config.header_timeout, client.head(url).send())
        .await
    {
        Err(_) => {
            return Err(ProbeFailure::Retry {
                reason: "HEAD request exceeded the header timeout".to_string(),
                timed_out: true,
            })
        }
        Ok(Err(error)) => {
            return Err(ProbeFailure::Retry {
                reason: format!("HEAD connection failed: {error}"),
                timed_out: false,
            })
        }
        Ok(Ok(response)) => response,
    };
    let head_status = head_response.status().as_u16();
    let (head_saw_ranges, need_get) = match head_status {
        200..=299 => {
            result.final_url = head_response.url().to_string();
            result.total = header_u64(&head_response, &CONTENT_LENGTH);
            result.etag = header_str(&head_response, &ETAG);
            result.last_modified = header_str(&head_response, &LAST_MODIFIED);
            result.content_type = header_str(&head_response, &CONTENT_TYPE);
            if let Some(disposition) = header_str(&head_response, &CONTENT_DISPOSITION) {
                result.filename = filename_from_content_disposition(&disposition);
            }
            let saw_ranges = header_accepts_bytes(&head_response, &ACCEPT_RANGES);
            // Range claims get behavioural confirmation, and a missing
            // length still needs a body-less look.
            (saw_ranges, saw_ranges || result.total.is_none())
        }
        405 | 501 => (false, true),
        408 | 429 | 500..=599 => {
            return Err(ProbeFailure::Retry {
                reason: format!("HEAD returned HTTP {head_status}"),
                timed_out: false,
            })
        }
        _ => {
            return Err(ProbeFailure::Fatal(EngineError::HttpStatus {
                status: head_status,
                url: url.to_string(),
            }))
        }
    };
    drop(head_response);

    // --- GET: confirm range support, catch whatever HEAD could not ------
    if need_get {
        let request = if head_saw_ranges {
            client.get(url).header(RANGE, "bytes=0-0")
        } else {
            client.get(url)
        };
        let response = match tokio::time::timeout(config.header_timeout, request.send()).await {
            Err(_) => {
                return Err(ProbeFailure::Retry {
                    reason: "GET probe exceeded the header timeout".to_string(),
                    timed_out: true,
                })
            }
            Ok(Err(error)) => {
                return Err(ProbeFailure::Retry {
                    reason: format!("GET probe connection failed: {error}"),
                    timed_out: false,
                })
            }
            Ok(Ok(response)) => response,
        };
        let status = response.status().as_u16();
        match status {
            200..=299 => {
                if result.final_url.is_empty() {
                    result.final_url = response.url().to_string();
                }
                if result.etag.is_none() {
                    result.etag = header_str(&response, &ETAG);
                }
                if result.last_modified.is_none() {
                    result.last_modified = header_str(&response, &LAST_MODIFIED);
                }
                if result.content_type.is_none() {
                    result.content_type = header_str(&response, &CONTENT_TYPE);
                }
                if result.filename.is_none()
                    && let Some(disposition) = header_str(&response, &CONTENT_DISPOSITION)
                {
                    result.filename = filename_from_content_disposition(&disposition);
                }
                if status == 206 {
                    // A 206 answer to the ranged probe is the behavioural
                    // proof that range support is real; only a header that
                    // already claimed support counts (servers that ignore
                    // Range while pretending otherwise fall through here).
                    if result.total.is_none()
                        && let Some((_, _, total)) = parse_content_range(&response)
                    {
                        result.total = total;
                    }
                    result.accepts_ranges = head_saw_ranges;
                } else {
                    result.accepts_ranges = false;
                    if result.total.is_none() {
                        result.total = header_u64(&response, &CONTENT_LENGTH);
                    }
                }
            }
            416 => {
                // `bytes 0-0` against an empty file reports `*/0`.
                match parse_content_range(&response) {
                    Some((_, _, Some(0))) => {
                        result.total = Some(0);
                    }
                    _ => {
                        return Err(ProbeFailure::Fatal(EngineError::protocol(
                            "range probe was rejected with an unexpected Content-Range",
                        )))
                    }
                }
            }
            408 | 429 | 500..=599 => {
                return Err(ProbeFailure::Retry {
                    reason: format!("GET probe returned HTTP {status}"),
                    timed_out: false,
                })
            }
            _ => {
                return Err(ProbeFailure::Fatal(EngineError::HttpStatus {
                    status,
                    url: url.to_string(),
                }))
            }
        }
        drop(response);
    }

    if result.final_url.is_empty() {
        result.final_url = url.to_string();
    }
    if result.filename.is_none() {
        result.filename = filename_from_url(&result.final_url);
    }
    Ok(result)
}

/// Parses `Content-Range` in both response forms:
/// `bytes 100-199/1000` and `bytes */1000`.
pub(crate) fn parse_content_range(
    response: &Response,
) -> Option<(Option<u64>, Option<u64>, Option<u64>)> {
    let value = header_str(response, &CONTENT_RANGE)?;
    parse_content_range_value(&value)
}

pub(crate) fn parse_content_range_value(
    value: &str,
) -> Option<(Option<u64>, Option<u64>, Option<u64>)> {
    let rest = value.trim().strip_prefix("bytes")?.trim();
    let (range_part, total_part) = rest.split_once('/')?;
    let range_part = range_part.trim();
    let total_part = total_part.trim();
    let total = match total_part {
        "*" => None,
        other => Some(other.parse::<u64>().ok()?),
    };
    if range_part == "*" {
        return Some((None, None, total));
    }
    let (start, end) = range_part.split_once('-')?;
    let start = start.trim().parse::<u64>().ok()?;
    let end = end.trim().parse::<u64>().ok()?;
    Some((Some(start), Some(end), total))
}

/// RFC 6266 filename extraction: `filename*=` (UTF-8 percent-encoded)
/// wins over `filename="…"`.
pub(crate) fn filename_from_content_disposition(value: &str) -> Option<String> {
    let mut plain: Option<String> = None;
    let mut extended: Option<String> = None;
    for part in value.split(';') {
        let part = part.trim();
        let Some((key, raw)) = part.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let raw = raw.trim().trim_matches('"');
        if key == "filename" {
            plain = Some(raw.to_string());
        } else if key == "filename*" && let Some(encoded) = raw.strip_prefix("UTF-8''") {
            extended = Some(percent_decode(encoded));
        }
    }
    let name = extended.or(plain)?;
    sanitize_filename(&name)
}

/// Falls back to the last meaningful path segment of the URL.
pub(crate) fn filename_from_url(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let segment = parsed.path_segments()?.rfind(|segment| !segment.is_empty())?;
    let decoded = percent_decode(segment);
    sanitize_filename(&decoded)
}

/// Keeps only the final path component and replaces characters that are
/// unsafe on Windows or in shells.
pub(crate) fn sanitize_filename(name: &str) -> Option<String> {
    let last = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = last
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*')
            {
                '_'
            } else {
                character
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.');
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned.to_string())
    }
}

/// Byte-level percent decoding (never panics on malformed sequences).
pub(crate) fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = hex_value(bytes[index + 1]);
            let low = hex_value(bytes[index + 2]);
            if let (Some(high), Some(low)) = (high, low) {
                output.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_content_range_forms() {
        assert_eq!(
            parse_content_range_value("bytes 0-0/1048576"),
            Some((Some(0), Some(0), Some(1_048_576)))
        );
        assert_eq!(
            parse_content_range_value("bytes */0"),
            Some((None, None, Some(0)))
        );
        assert_eq!(
            parse_content_range_value("bytes */*"),
            Some((None, None, None))
        );
        assert_eq!(parse_content_range_value("items 0-1/2"), None);
        assert_eq!(parse_content_range_value("bytes x-1/2"), None);
        assert_eq!(parse_content_range_value("bytes 0-1/x"), None);
    }

    #[test]
    fn extracts_filenames_safely() {
        assert_eq!(
            filename_from_content_disposition(r#"attachment; filename="report.pdf""#).as_deref(),
            Some("report.pdf")
        );
        assert_eq!(
            filename_from_content_disposition("attachment; filename*=UTF-8''%E2%82%AC.txt")
                .as_deref(),
            Some("€.txt")
        );
        assert_eq!(
            filename_from_content_disposition(r#"attachment; filename="../../evil.exe""#).as_deref(),
            Some("evil.exe")
        );
        assert_eq!(
            filename_from_content_disposition("attachment; filename=\"..\"").as_deref(),
            None
        );
        assert_eq!(
            filename_from_url("https://example.com/files/payload%20one.bin?token=1").as_deref(),
            Some("payload one.bin")
        );
        assert_eq!(filename_from_url("https://example.com/").as_deref(), None);
    }

    #[test]
    fn decodes_percent_sequences_without_panicking() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("€"), "€");
        assert_eq!(percent_decode("%E2%82%AC"), "€");
    }
}
