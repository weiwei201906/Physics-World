//! Minimal S3 (SigV4) client for RustFS.
//!
//! The question bundle and its images live in object storage. This client
//! reads the bundle at startup and publishes locally bundled media so the
//! gateway can serve `/media/*` straight from RustFS. Only plain HTTP
//! endpoints are supported; internal object storage is not exposed to
//! browsers, so TLS is terminated elsewhere when needed.

use std::time::Duration;

use bytes::Bytes;
use chrono::Utc;
use common::error::{AppError, Result};
use hmac::{Hmac, Mac};
use http_body_util::{BodyExt, Full};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::config::RustfsSettings;

type HmacSha256 = Hmac<Sha256>;

pub async fn fetch_json(settings: &RustfsSettings, timeout: Duration) -> Result<String> {
    let path = format!("/{}/{}", settings.bucket, settings.object);
    let request = signed_request(settings, http::Method::GET, &path, &[], &[], None)?;
    let body = tokio::time::timeout(timeout, send(request))
        .await
        .map_err(|_| AppError::internal("rustfs request timed out"))??;
    String::from_utf8(body.to_vec())
        .map_err(|error| AppError::invalid(format!("rustfs payload is not UTF-8: {error}")))
}

/// Creates the bucket and grants anonymous `GetObject` on every object in it.
///
/// Question images are public content, and the gateway proxies them without
/// credentials, so the bucket must allow anonymous reads. The policy is
/// limited to a dedicated media bucket so other buckets stay private.
pub async fn ensure_public_bucket(
    settings: &RustfsSettings,
    bucket: &str,
    timeout: Duration,
) -> Result<()> {
    let bucket_path = format!("/{bucket}");
    let create = signed_request(settings, http::Method::PUT, &bucket_path, &[], &[], None)?;
    let status = tokio::time::timeout(timeout, send_status(create))
        .await
        .map_err(|_| AppError::internal("rustfs bucket request timed out"))??;
    if !status.is_success() && status.as_u16() != 409 {
        return Err(AppError::internal(format!(
            "rustfs refused to create bucket {bucket}: {status}"
        )));
    }

    let policy = json!({
        "Version": "2012-10-17",
        "Statement": [{
            "Effect": "Allow",
            "Principal": "*",
            "Action": ["s3:GetObject"],
            "Resource": [format!("arn:aws:s3:::{bucket}/*")],
        }],
    })
    .to_string();
    let request = signed_request(
        settings,
        http::Method::PUT,
        &bucket_path,
        &[("policy", "")],
        policy.as_bytes(),
        Some("application/json"),
    )?;
    let _policy = tokio::time::timeout(timeout, send(request))
        .await
        .map_err(|_| AppError::internal("rustfs policy request timed out"))??;
    Ok(())
}

pub async fn put_object(
    settings: &RustfsSettings,
    bucket: &str,
    key: &str,
    body: Bytes,
    content_type: &str,
    timeout: Duration,
) -> Result<()> {
    let path = format!("/{bucket}/{key}");
    let request = signed_request(
        settings,
        http::Method::PUT,
        &path,
        &[],
        &body,
        Some(content_type),
    )?;
    let _response = tokio::time::timeout(timeout, send(request))
        .await
        .map_err(|_| AppError::internal("rustfs upload timed out"))??;
    Ok(())
}

fn signed_request(
    settings: &RustfsSettings,
    method: http::Method,
    path: &str,
    query: &[(&str, &str)],
    body: &[u8],
    content_type: Option<&str>,
) -> Result<http::Request<Full<Bytes>>> {
    let host = settings
        .endpoint
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/')
        .to_owned();
    let base = settings.endpoint.trim_end_matches('/');

    let now = Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();
    let payload_hash = sha256_hex(body);

    let canonical_query = canonical_query(query);
    let mut headers = vec![
        ("host", host.clone()),
        ("x-amz-content-sha256", payload_hash.clone()),
        ("x-amz-date", amz_date.clone()),
    ];
    if let Some(content_type) = content_type {
        headers.push(("content-type", content_type.to_owned()));
    }
    headers.sort_by(|left, right| left.0.cmp(right.0));
    let signed_headers = headers
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(";");
    let canonical_headers = headers
        .iter()
        .map(|(name, value)| format!("{name}:{value}\n"))
        .collect::<String>();

    let canonical_request = format!(
        "{}\n{}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{payload_hash}",
        method.as_str(),
        encode_path(path),
    );
    let scope = format!("{date_stamp}/{}/s3/aws4_request", settings.region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let signing_key = signing_key(&settings.secret_key, &date_stamp, &settings.region);
    let signature = hex::encode(hmac(&signing_key, string_to_sign.as_bytes()));
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, \
         Signature={signature}",
        settings.access_key
    );

    let url = if canonical_query.is_empty() {
        format!("{base}{path}")
    } else {
        format!("{base}{path}?{canonical_query}")
    };
    let mut builder = http::Request::builder()
        .method(method)
        .uri(&url)
        .header("authorization", authorization);
    for (name, value) in headers.iter() {
        if *name != "host" {
            builder = builder.header(*name, value.as_str());
        }
    }
    builder
        .body(Full::new(Bytes::from(body.to_vec())))
        .map_err(|error| AppError::internal(format!("build rustfs request: {error}")))
}

async fn send(request: http::Request<Full<Bytes>>) -> Result<Bytes> {
    let client: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new()).build_http();
    let response = client
        .request(request)
        .await
        .map_err(|error| AppError::internal(format!("rustfs request failed: {error}")))?;
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .map_err(|error| AppError::internal(format!("read rustfs response: {error}")))?
        .to_bytes();
    if !status.is_success() {
        let snippet = String::from_utf8_lossy(&body);
        let snippet: String = snippet.chars().take(200).collect();
        return Err(AppError::internal(format!(
            "rustfs returned {status}: {snippet}"
        )));
    }
    Ok(body)
}

async fn send_status(request: http::Request<Full<Bytes>>) -> Result<http::StatusCode> {
    let client: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new()).build_http();
    let response = client
        .request(request)
        .await
        .map_err(|error| AppError::internal(format!("rustfs request failed: {error}")))?;
    Ok(response.status())
}

fn canonical_query(query: &[(&str, &str)]) -> String {
    let mut pairs: Vec<(String, String)> = query
        .iter()
        .map(|(name, value)| (encode(name), encode(value)))
        .collect();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// Percent-encodes everything except the RFC 3986 unreserved characters.
fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// Canonical URIs keep path separators literal and encode every segment.
fn encode_path(path: &str) -> String {
    path.split('/').map(encode).collect::<Vec<_>>().join("/")
}

fn signing_key(secret: &str, date_stamp: &str, region: &str) -> Vec<u8> {
    let k_date = hmac(format!("AWS4{secret}").as_bytes(), date_stamp.as_bytes());
    let k_region = hmac(&k_date, region.as_bytes());
    let k_service = hmac(&k_region, b"s3");
    hmac(&k_service, b"aws4_request")
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_payload_hash_matches_known_vector() {
        assert_eq!(
            sha256_hex(&[]),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn signing_key_is_stable() {
        let key = signing_key("secret", "20260101", "us-east-1");
        assert_eq!(key.len(), 32);
        assert_eq!(key, signing_key("secret", "20260101", "us-east-1"));
    }

    #[test]
    fn canonical_query_sorts_and_encodes() {
        assert_eq!(canonical_query(&[("b", "2"), ("a", "1")]), "a=1&b=2");
        assert_eq!(canonical_query(&[("policy", "")]), "policy=");
        assert_eq!(
            encode_path("/bucket/media/a b.svg"),
            "/bucket/media/a%20b.svg"
        );
    }
}
