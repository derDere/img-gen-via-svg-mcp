//! Remote fetching for `svg_url` and for resources a document references.
//!
//! Both are off unless the operator turns them on: a server that fetches
//! whatever a rendered document points at is a request-forgery tool, and the
//! decision to accept that belongs to whoever runs the process.

use crate::config::Config;
use crate::error::{ErrorCode, Result, ToolError};
use serde_json::json;

/// Refuses when remote input is disabled, and otherwise fetches the URL.
pub fn fetch(url: &str, config: &Config) -> Result<Vec<u8>> {
    if !config.remote_input {
        return Err(disabled(url, "IMG_SVG_MCP_REMOTE_INPUT"));
    }
    check_host(url, config)?;
    fetch_unchecked(url, config)
}

/// Refuses when remote references are disabled, and otherwise fetches the URL.
pub fn fetch_reference(url: &str, config: &Config) -> Result<Vec<u8>> {
    if !config.remote_svg_references {
        return Err(disabled(url, "IMG_SVG_MCP_REMOTE_SVG_REFERENCES"));
    }
    check_host(url, config)?;
    fetch_unchecked(url, config)
}

fn disabled(url: &str, variable: &str) -> ToolError {
    ToolError::new(
        ErrorCode::RemoteDisabled,
        format!("Remote access is disabled; {url} was not fetched."),
    )
    .with_detail(json!({ "url": url }))
    .with_hint(format!("Start the server with {variable}=true to allow it."))
}

/// Checks the host against the operator's allowlist.
fn check_host(url: &str, config: &Config) -> Result<()> {
    if config.remote_allowlist.is_empty() {
        return Ok(());
    }
    let host = host_of(url).unwrap_or_default();
    let permitted = config.remote_allowlist.iter().any(|pattern| {
        pattern
            .strip_prefix("*.")
            .map(|suffix| host == suffix || host.ends_with(&format!(".{suffix}")))
            .unwrap_or(pattern == &host)
    });
    if permitted {
        Ok(())
    } else {
        Err(ToolError::new(
            ErrorCode::RemoteDisabled,
            format!("Host {host} is not in the remote allowlist."),
        )
        .with_detail(
            json!({ "url": url, "host": host, "remote_allowlist": config.remote_allowlist }),
        ))
    }
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
    Some(host.split(':').next().unwrap_or(host).to_ascii_lowercase())
}

#[cfg(feature = "remote")]
fn fetch_unchecked(url: &str, config: &Config) -> Result<Vec<u8>> {
    use std::io::Read;

    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .user_agent(concat!("img-gen-via-svg-mcp/", env!("CARGO_PKG_VERSION")))
        .build()
        .new_agent();

    let response = agent.get(url).call().map_err(|e| {
        ToolError::new(ErrorCode::RemoteFailed, format!("Fetching {url} failed: {e}"))
    })?;

    let status = response.status().as_u16();
    let limit = config.remote_max_bytes;
    let mut body = Vec::new();
    response.into_body().into_reader().take(limit + 1).read_to_end(&mut body).map_err(|e| {
        ToolError::new(ErrorCode::RemoteFailed, format!("Reading {url} failed: {e}"))
    })?;

    if body.len() as u64 > limit {
        return Err(ToolError::new(
            ErrorCode::RemoteFailed,
            format!("{url} exceeds the remote size limit of {limit} bytes."),
        )
        .with_detail(json!({ "url": url, "status": status, "remote_max_bytes": limit })));
    }
    Ok(body)
}

#[cfg(not(feature = "remote"))]
fn fetch_unchecked(url: &str, _config: &Config) -> Result<Vec<u8>> {
    Err(ToolError::new(
        ErrorCode::RemoteDisabled,
        format!("This build has no HTTP client, so {url} cannot be fetched."),
    )
    .with_hint("Rebuild with the `remote` feature enabled."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_the_host() {
        assert_eq!(host_of("https://example.com/a.svg").as_deref(), Some("example.com"));
        assert_eq!(host_of("http://user@Example.COM:8080/a").as_deref(), Some("example.com"));
    }

    #[test]
    fn refuses_when_disabled() {
        let config = Config::default();
        let error = fetch("https://example.com/a.svg", &config).unwrap_err();
        assert_eq!(error.code, ErrorCode::RemoteDisabled);
    }

    #[test]
    fn honours_a_wildcard_allowlist() {
        let config = Config { remote_allowlist: vec!["*.example.com".into()], ..Config::default() };
        assert!(check_host("https://cdn.example.com/a.svg", &config).is_ok());
        assert!(check_host("https://elsewhere.test/a.svg", &config).is_err());
    }
}
