//! Per-request headers and credentials.
//!
//! A download sometimes has to look like the browser that started it: a
//! referer, cookies, a user agent, HTTP basic credentials, or a few arbitrary
//! headers a site insists on. Everything here travels on the task row, is
//! written into the sidecar manifest, and is applied to every request the
//! engine makes — the probe and the segment transfers alike.
//!
//! The probe walks a fallback ladder when a server rejects a request
//! ([`HeaderProfile`]); the profile that succeeded is recorded on the probe
//! result and reused for the transfer, so the bytes that arrive are the ones
//! the headers that worked asked for.

use reqwest::RequestBuilder;
use reqwest::header::{AUTHORIZATION, COOKIE, HeaderName, HeaderValue, REFERER, USER_AGENT};
use serde::{Deserialize, Serialize};

use crate::base64;

/// HTTP basic credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BasicAuth {
    pub username: String,
    pub password: String,
}

/// Every header a request may carry besides the URL.
///
/// `extra` holds anything a site asks for that has no dedicated field. Pair
/// order is preserved so a site that reads a header twice in a row keeps
/// seeing it twice.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestHeaders {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cookies: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub basic_auth: Option<BasicAuth>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<(String, String)>,
}

impl RequestHeaders {
    /// Whether the task asked for no identifying header at all.
    pub fn is_empty(&self) -> bool {
        self.referer.is_none()
            && self.cookies.is_none()
            && self.user_agent.is_none()
            && self.basic_auth.is_none()
            && self.extra.is_empty()
    }

    /// The cookie string as it will be sent, trimmed and dropped when blank.
    pub fn cookie_value(&self) -> Option<&str> {
        self.cookies
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    /// Applies these headers to a request builder.
    ///
    /// A header that is not valid HTTP is skipped rather than allowed to
    /// panic the transfer: a stray control character in a pasted cookie
    /// string must not take the whole download down with it.
    pub fn apply(&self, mut builder: RequestBuilder) -> RequestBuilder {
        if let Some(value) = self.cookie_value() {
            builder = push_header(builder, COOKIE.as_str(), value);
        }
        if let Some(referer) = self
            .referer
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            builder = push_header(builder, REFERER.as_str(), referer);
        }
        if let Some(agent) = self
            .user_agent
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            builder = push_header(builder, USER_AGENT.as_str(), agent);
        }
        if let Some(auth) = &self.basic_auth {
            let token = base64::encode(format!("{}:{}", auth.username, auth.password).as_bytes());
            builder = push_header(builder, AUTHORIZATION.as_str(), &format!("Basic {token}"));
        }
        for (name, value) in &self.extra {
            builder = push_header(builder, name, value);
        }
        builder
    }

    /// Drops cookies but keeps the referer and the user agent.
    pub fn without_cookies(&self) -> Self {
        Self {
            referer: self.referer.clone(),
            cookies: None,
            user_agent: self.user_agent.clone(),
            basic_auth: self.basic_auth.clone(),
            extra: self.extra.clone(),
        }
    }

    /// Drops cookies, referer, and the user agent: the identity-free profile.
    /// Arbitrary extra headers survive, because a site that needs a specific
    /// token is not asking to be anonymous, only to be reached.
    pub fn without_identity(&self) -> Self {
        Self {
            referer: None,
            cookies: None,
            user_agent: None,
            basic_auth: self.basic_auth.clone(),
            extra: self.extra.clone(),
        }
    }
}

/// Which subset of a task's headers a request actually carried. The probe
/// ladder records the one that worked so the transfer reuses it exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeaderProfile {
    /// Everything the task asked for.
    #[default]
    Full,
    /// Cookies removed; referer and user agent kept.
    NoCookies,
    /// No cookies, referer, or user agent.
    NoIdentity,
}

impl HeaderProfile {
    /// The header set this profile sends.
    pub fn apply(&self, headers: &RequestHeaders) -> RequestHeaders {
        match self {
            Self::Full => headers.clone(),
            Self::NoCookies => headers.without_cookies(),
            Self::NoIdentity => headers.without_identity(),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::NoCookies => "no_cookies",
            Self::NoIdentity => "no_identity",
        }
    }
}

/// Adds one header when both its name and value are legal HTTP.
fn push_header(builder: RequestBuilder, name: &str, value: &str) -> RequestBuilder {
    let (Ok(name), Ok(value)) = (
        HeaderName::from_bytes(name.as_bytes()),
        HeaderValue::from_str(value),
    ) else {
        return builder;
    };
    builder.header(name, value)
}

/// RFC 7617 basic credentials: `base64(user:password)`.
pub fn basic_token(username: &str, password: &str) -> String {
    base64::encode(format!("{username}:{password}").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_credentials_are_encoded_with_a_colon() {
        assert_eq!(basic_token("user", "pass"), "dXNlcjpwYXNz");
        assert_eq!(basic_token("", ""), "Og==");
    }

    #[test]
    fn empty_headers_are_detected() {
        assert!(RequestHeaders::default().is_empty());
        assert!(
            !RequestHeaders {
                extra: vec![("X-Keep".to_string(), "1".to_string())],
                ..Default::default()
            }
            .is_empty(),
            "an extra header is still an identifying header"
        );
    }

    #[test]
    fn the_identity_ladder_removes_one_thing_at_a_time() {
        let headers = RequestHeaders {
            referer: Some("https://example.com/watch".to_string()),
            cookies: Some("a=1; b=2".to_string()),
            user_agent: Some("RAPIDS/1".to_string()),
            ..Default::default()
        };
        let without_cookies = HeaderProfile::NoCookies.apply(&headers);
        assert_eq!(
            without_cookies.cookies, None,
            "the cookies profile must drop cookies"
        );
        assert_eq!(
            without_cookies.referer, headers.referer,
            "dropping cookies must not touch the referer"
        );
        assert_eq!(
            without_cookies.user_agent, headers.user_agent,
            "dropping cookies must not touch the user agent"
        );
        let anonymous = HeaderProfile::NoIdentity.apply(&headers);
        assert_eq!(anonymous.cookies, None);
        assert_eq!(anonymous.referer, None);
        assert_eq!(anonymous.user_agent, None);
        assert_eq!(
            HeaderProfile::Full.apply(&headers),
            headers,
            "the full profile must send everything"
        );
    }

    #[test]
    fn blank_optional_headers_are_not_sent() {
        assert_eq!(
            RequestHeaders {
                referer: Some("   ".to_string()),
                cookies: Some(String::new()),
                ..Default::default()
            }
            .cookie_value(),
            None
        );
    }
}
