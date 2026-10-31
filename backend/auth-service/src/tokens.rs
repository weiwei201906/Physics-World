//! RS256 access tokens, opaque refresh tokens and the public JWKS document.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use rsa::pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey, LineEnding};
use rsa::traits::PublicKeyParts;
use rsa::RsaPrivateKey;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use common::defaults;
use common::error::{AppError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claims {
    pub sub: String,
    pub roles: Vec<String>,
    pub email: String,
    pub iss: String,
    pub aud: String,
    pub iat: i64,
    pub exp: i64,
}

pub struct KeyRing {
    encoding: EncodingKey,
    public_pem: String,
    kid: String,
    issuer: String,
    audience: String,
    modulus: String,
    exponent: String,
}

impl KeyRing {
    /// Generates an ephemeral RSA key. Production deploys supply a stable PEM
    /// through `AUTH_JWT_PRIVATE_PEM`.
    pub fn generate(kid: &str) -> Result<Self> {
        let mut rng = rand::thread_rng();
        let private = RsaPrivateKey::new(&mut rng, 2048)
            .map_err(|error| AppError::config("jwt.private_key", error.to_string()))?;
        let pem = private
            .to_pkcs8_pem(LineEnding::LF)
            .map_err(|error| AppError::config("jwt.private_key", error.to_string()))?;
        Self::from_pkcs8_pem(&pem, kid)
    }

    pub fn from_pkcs8_pem(pem: &str, kid: &str) -> Result<Self> {
        let encoding = EncodingKey::from_rsa_pem(pem.as_bytes())
            .map_err(|error| AppError::config("jwt.private_key", error.to_string()))?;
        let private = RsaPrivateKey::from_pkcs8_pem(pem)
            .map_err(|error| AppError::config("jwt.private_key", error.to_string()))?;
        let public = private.to_public_key();
        let public_pem = public
            .to_public_key_pem(LineEnding::LF)
            .map_err(|error| AppError::config("jwt.public_key", error.to_string()))?;
        Ok(Self {
            encoding,
            public_pem,
            kid: kid.to_string(),
            issuer: defaults::AUTH_JWT_ISSUER.to_string(),
            audience: "user-api".to_string(),
            modulus: URL_SAFE_NO_PAD.encode(public.n().to_bytes_be()),
            exponent: URL_SAFE_NO_PAD.encode(public.e().to_bytes_be()),
        })
    }

    pub fn kid(&self) -> &str {
        &self.kid
    }

    pub fn public_pem(&self) -> &str {
        &self.public_pem
    }

    pub fn issue_access_token(
        &self,
        user_id: Uuid,
        email: &str,
        roles: &[String],
        now: DateTime<Utc>,
        ttl: Duration,
    ) -> Result<(String, i64)> {
        let claims = Claims {
            sub: user_id.to_string(),
            roles: roles.to_vec(),
            email: email.to_string(),
            iss: self.issuer.clone(),
            aud: self.audience.clone(),
            iat: now.timestamp(),
            exp: (now + ttl).timestamp(),
        };
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.kid.clone());
        let token = encode(&header, &claims, &self.encoding).map_err(|error| {
            AppError::internal(format!("access token encoding failed: {error}"))
        })?;
        Ok((token, ttl.num_seconds()))
    }

    /// Validates an access token so the gateway can trust its identity claims.
    pub fn verify_access_token(&self, token: &str) -> Result<Claims> {
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&["user-api"]);
        validation.set_issuer(&[defaults::AUTH_JWT_ISSUER]);
        let key = DecodingKey::from_rsa_pem(self.public_pem.as_bytes())
            .map_err(|error| AppError::config("jwt.public_key", error.to_string()))?;
        decode::<Claims>(token, &key, &validation)
            .map(|data| data.claims)
            .map_err(|_| AppError::unauthenticated("invalid access token"))
    }

    pub fn generate_refresh_token(&self, bytes: usize) -> Result<String> {
        let mut buffer = vec![0u8; bytes.max(16)];
        rand::thread_rng().fill_bytes(&mut buffer);
        Ok(URL_SAFE_NO_PAD.encode(buffer))
    }

    pub fn jwks(&self) -> serde_json::Value {
        serde_json::json!({
            "keys": [{
                "kty": "RSA",
                "use": "sig",
                "alg": "RS256",
                "kid": self.kid,
                "n": self.modulus,
                "e": self.exponent,
            }]
        })
    }
}

pub fn hash_refresh_token(raw: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(raw.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{decode, DecodingKey, Validation};

    fn keys() -> KeyRing {
        KeyRing::generate(defaults::AUTH_JWT_KEY_ID).expect("key ring")
    }

    #[test]
    fn access_token_round_trips_with_rs256() {
        let keys = keys();
        let now = Utc::now();
        let (token, ttl) = keys
            .issue_access_token(
                Uuid::nil(),
                "player@example.com",
                &["student".to_string()],
                now,
                Duration::seconds(600),
            )
            .expect("token");
        assert_eq!(ttl, 600);

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&["user-api"]);
        validation.set_issuer(&[defaults::AUTH_JWT_ISSUER]);
        let decoded = decode::<Claims>(
            &token,
            &DecodingKey::from_rsa_pem(keys.public_pem().as_bytes()).expect("decoding key"),
            &validation,
        )
        .expect("decode");
        assert_eq!(decoded.claims.sub, Uuid::nil().to_string());
        assert_eq!(decoded.claims.exp, now.timestamp() + 600);
        assert_eq!(
            keys.verify_access_token(&token).expect("verify").sub,
            Uuid::nil().to_string()
        );
    }

    #[test]
    fn verify_access_token_rejects_invalid_tokens() {
        let key_ring = keys();
        assert!(key_ring.verify_access_token("not-a-token").is_err());

        let other = keys();
        let now = Utc::now();
        let (token, _) = other
            .issue_access_token(
                Uuid::nil(),
                "player@example.com",
                &["student".to_string()],
                now,
                Duration::seconds(600),
            )
            .expect("token");
        assert!(key_ring.verify_access_token(&token).is_err());
    }

    #[test]
    fn jwks_exposes_rsa_modulus_and_exponent() {
        let jwks = keys().jwks();
        let key = &jwks["keys"][0];
        assert_eq!(key["kty"], "RSA");
        assert_eq!(key["alg"], "RS256");
        assert!(!key["n"].as_str().unwrap_or_default().is_empty());
        assert!(!key["e"].as_str().unwrap_or_default().is_empty());
    }

    #[test]
    fn refresh_tokens_are_unique_and_hashed_stably() {
        let keys = keys();
        let first = keys.generate_refresh_token(32).expect("token");
        let second = keys.generate_refresh_token(32).expect("token");
        assert_ne!(first, second);
        assert_eq!(hash_refresh_token(&first), hash_refresh_token(&first));
        assert_ne!(hash_refresh_token(&first), hash_refresh_token(&second));
    }
}
