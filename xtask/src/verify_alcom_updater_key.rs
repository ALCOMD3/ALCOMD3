use crate::sign_alcom_updater::{secret_key, updater_env};
use crate::verify_alcom_updater_json::read_public_key;
use anyhow::{Context, Result};
use minisign::{PublicKey, SecretKey};
use std::io::Cursor;
use std::path::PathBuf;

const KEY_CHECK_CHALLENGE: &[u8] = b"ALCOMD3 updater signing key self-check";

/// Verify that the configured updater private key, password, and embedded public key match.
#[derive(clap::Parser)]
pub struct Command {
    /// Embedded updater public key used by the GUI.
    #[arg(long, default_value = "vrc-get-gui/src/updater-public-key.txt")]
    public_key: PathBuf,
}

impl crate::Command for Command {
    fn run(self) -> Result<i32> {
        let private_key = updater_env(crate::release_common::UPDATER_PRIVATE_KEY_ENV)?;
        let password = updater_env(crate::release_common::UPDATER_PRIVATE_KEY_PASSWORD_ENV)?;
        let secret_key = secret_key(&private_key, &password)?;
        let public_key = read_public_key(&self.public_key)?;

        verify_key_pair(&secret_key, &public_key)?;
        println!("updater signing key verification passed");
        Ok(0)
    }
}

fn verify_key_pair(secret_key: &SecretKey, public_key: &PublicKey) -> Result<()> {
    let signature = minisign::sign(
        Some(public_key),
        secret_key,
        Cursor::new(KEY_CHECK_CHALLENGE),
        Some("purpose:key-verification"),
        Some("ALCOMD3 updater key verification"),
    )
    .context("updater private key does not match the embedded public key")?;

    minisign::verify(
        public_key,
        &signature,
        Cursor::new(KEY_CHECK_CHALLENGE),
        true,
        false,
        false,
    )
    .context("updater signing key self-check signature verification failed")
}

#[cfg(test)]
mod tests {
    use super::verify_key_pair;
    use crate::sign_alcom_updater::secret_key;
    use base64::Engine;
    use minisign::KeyPair;

    fn encrypted_private_key(password: &str) -> (String, minisign::PublicKey) {
        let pair = KeyPair::generate_encrypted_keypair(Some(password.to_string())).unwrap();
        let secret_box = pair.sk.to_box(Some("test updater key")).unwrap();
        let encoded = base64::engine::general_purpose::STANDARD.encode(secret_box.to_string());
        (encoded, pair.pk)
    }

    #[test]
    fn matching_encrypted_key_and_password_pass() {
        let password = "correct-password";
        let (private_key, public_key) = encrypted_private_key(password);
        let secret_key = secret_key(&private_key, password).unwrap();
        verify_key_pair(&secret_key, &public_key).unwrap();
    }

    #[test]
    fn wrong_password_is_rejected_without_echoing_secret_values() {
        let correct_password = "correct-password";
        let wrong_password = "wrong-password";
        let (private_key, _) = encrypted_private_key(correct_password);
        let error = secret_key(&private_key, wrong_password).unwrap_err();
        let message = format!("{error:#}");

        assert!(message.contains("incorrect updater private key password"));
        assert!(!message.contains(&private_key));
        assert!(!message.contains(correct_password));
        assert!(!message.contains(wrong_password));
    }

    #[test]
    fn malformed_private_key_is_rejected_without_echoing_it() {
        let private_key = "not-valid-base64";
        let error = secret_key(private_key, "password").unwrap_err();
        let message = format!("{error:#}");

        assert!(message.contains("failed to decode base64 secret key"));
        assert!(!message.contains(private_key));
    }

    #[test]
    fn mismatched_public_key_is_rejected() {
        let first = KeyPair::generate_unencrypted_keypair().unwrap();
        let second = KeyPair::generate_unencrypted_keypair().unwrap();
        let error = verify_key_pair(&first.sk, &second.pk).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("does not match the embedded public key")
        );
    }
}
