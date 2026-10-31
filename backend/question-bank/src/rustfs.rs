//! Minimal S3 (SigV4) client for reading the question bundle from RustFS.
//!
//! Only plain HTTP endpoints are supported; internal object storage is not
//! exposed to browsers, so TLS is terminated elsewhere when needed.

use std::time::Duration;

use bytes::Bytes;
use chrono::Utc;
use common::error::{AppError, Result};
use hmac::{Hmac, Mac};
use http_body_util::{BodyExt, Full};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use sha2::{Digest, Sha256};

use crate::config::RustfsSettings;

type HmacSha256 = Hmac<Sha256>;

pub async fn fetch_json(settings: &RustfsSettings, timeout: Duration) -> Result<String> {
    tokio::time::timeout(timeout, get_object(settings))
        .await
        .map_err(|_| AppError::internal("rustfs request timed out"))?
}

async fn get_object(settings: &RustfsSettings) -> Result<String> {
    let base: http::Uri = settings
        .endpoint
        .parse()
        .map_err(|error| AppError::invalid(format!("invalid rustfs endpoint: {error}")))?;
    let host = base
        .authority()
        .ok_or_else(|| AppError::invalid("rustfs endpoint has no host"))?
        .as_str()
        .to_owned();
    let path = format!("/{}/{}", settings.bucket, settings.object);
    let url = format!("{}{}", settings.endpoint.trim_end_matches('/'), path);

    let now = Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();
    let payload_hash = sha256_hex(&[]);

    let canonical_headers =
        format!("host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n");
    let signed_headers = "host;x-amz-content-sha256;x-amz-date";
    let canonical_request =
        format!("GET\n{path}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}");
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

    let request = http::Request::builder()
        .method(http::Method::GET)
        .uri(&url)
        .header("x-amz-content-sha256", &payload_hash)
        .header("x-amz-date", &amz_date)
        .header("authorization", authorization)
        .body(Full::new(Bytes::new()))
        .map_err(|error| AppError::internal(format!("build rustfs request: {error}")))?;

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
    String::from_utf8(body.to_vec())
        .map_err(|error| AppError::invalid(format!("rustfs payload is not UTF-8: {error}")))
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
}
