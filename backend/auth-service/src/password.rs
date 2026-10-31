//! Argon2id password hashing configured by [`AuthSettings`].

use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::{Algorithm, Argon2, Params, Version};
use common::error::{AppError, Result};
use common::settings::AuthSettings;

fn hasher(settings: &AuthSettings) -> Result<Argon2<'static>> {
    let params = Params::new(
        settings.argon2_memory_kb,
        settings.argon2_iterations,
        settings.argon2_parallelism,
        None,
    )
    .map_err(|error| AppError::config("argon2", error.to_string()))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

pub fn hash_password(settings: &AuthSettings, password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    hasher(settings)?
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| AppError::internal(format!("password hashing failed: {error}")))
}

pub fn verify_password(settings: &AuthSettings, password: &str, encoded: &str) -> Result<bool> {
    let parsed = PasswordHash::new(encoded)
        .map_err(|error| AppError::internal(format!("stored password hash is invalid: {error}")))?;
    Ok(hasher(settings)?
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::config::ConfigLoader;
    use std::collections::HashMap;

    fn settings() -> AuthSettings {
        let mut settings = AuthSettings::from_config(&ConfigLoader::new("auth", HashMap::new()));
        settings.argon2_memory_kb = 4096;
        settings.argon2_iterations = 1;
        settings.argon2_parallelism = 1;
        settings
    }

    #[test]
    fn hash_then_verify_round_trip() {
        let settings = settings();
        let hash = hash_password(&settings, "correct horse").expect("hash");
        assert!(verify_password(&settings, "correct horse", &hash).expect("verify"));
        assert!(!verify_password(&settings, "wrong horse", &hash).expect("verify"));
    }

    #[test]
    fn hashes_are_salted_independently() {
        let settings = settings();
        let first = hash_password(&settings, "same password").expect("hash");
        let second = hash_password(&settings, "same password").expect("hash");
        assert_ne!(first, second);
    }
}
