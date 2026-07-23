// ALCOMD3_UPDATER_PRIVATE_KEY
// ALCOMD3_UPDATER_PRIVATE_KEY_PASSWORD

use crate::release_common::{
    UPDATER_PRIVATE_KEY_ENV, UPDATER_PRIVATE_KEY_PASSWORD_ENV, UpdaterSignaturePurpose,
};
use anyhow::{Context, Result};
use base64::Engine;
use minisign::{SecretKey, SignatureBox};
use std::fs;
use std::path::{Path, PathBuf};

/// Signs updater artifact
#[derive(clap::Parser)]
pub struct Command {
    /// Identifies whether the signature is for a local test build or an official release.
    #[arg(long, value_enum, default_value_t = UpdaterSignaturePurpose::LocalTest)]
    purpose: UpdaterSignaturePurpose,

    #[arg()]
    file: PathBuf,
}

impl crate::Command for Command {
    fn run(self) -> anyhow::Result<i32> {
        let private_key = updater_env(UPDATER_PRIVATE_KEY_ENV)?;
        let password = updater_env(UPDATER_PRIVATE_KEY_PASSWORD_ENV)?;

        let signature = sign_file(
            &secret_key(&private_key, &password)?,
            &self.file,
            self.purpose,
        )
        .with_context(|| "failed to sign file")?;

        let signature_path = self.file.with_added_extension("sig");

        let encoded_signature =
            base64::engine::general_purpose::STANDARD.encode(signature.to_string());

        fs::write(&signature_path, &encoded_signature).with_context(|| {
            format!(
                "failed to write signature file: {}",
                signature_path.display()
            )
        })?;

        println!(
            "Your file was signed successfully, You can find the signature here:\n\
            {signature_path}\n
            \n\
            Public signature:\n\
            {encoded_signature}\
            \n
            \n\
            Make sure to include this into the signature field of your update server.",
            signature_path = signature_path.display(),
        );

        Ok(0)
    }
}

pub(crate) fn updater_env(name: &str) -> Result<String> {
    let value =
        std::env::var(name).with_context(|| format!("Required environment variable {name}"))?;
    ensure_updater_env_value(name, &value)?;
    Ok(value)
}

fn ensure_updater_env_value(name: &str, value: &str) -> Result<()> {
    anyhow::ensure!(
        !normalized_secret_value(value).is_empty(),
        "Required environment variable {name} is empty"
    );
    Ok(())
}

pub(crate) fn secret_key(private_key: &str, password: &str) -> Result<SecretKey> {
    let private_key = normalized_secret_value(private_key);
    let password = normalized_secret_value(password);
    let decoded_secret = base64::engine::general_purpose::STANDARD
        .decode(private_key)
        .map_err(anyhow::Error::from)
        .and_then(|x| String::from_utf8(x).map_err(anyhow::Error::from))
        .context("failed to decode base64 secret key")?;

    let sk_box = minisign::SecretKeyBox::from_string(&decoded_secret)
        .context("failed to load updater private key")?;
    let sk = sk_box
        .into_secret_key(Some(password.into()))
        .context("incorrect updater private key password")?;
    Ok(sk)
}

fn normalized_secret_value(value: &str) -> &str {
    value
        .strip_prefix('\u{feff}')
        .unwrap_or(value)
        .trim_end_matches(['\r', '\n'])
}

pub fn sign_file(
    secret_key: &SecretKey,
    bin_path: &Path,
    purpose: UpdaterSignaturePurpose,
) -> Result<SignatureBox> {
    let trusted_comment = format!(
        "timestamp:{}\tfile:{}\tpurpose:{}",
        unix_timestamp(),
        bin_path.file_name().unwrap().to_string_lossy(),
        purpose,
    );

    let data_reader = fs::File::open(bin_path).context("failed to open data file")?;

    let signature_box = minisign::sign(
        None,
        secret_key,
        data_reader,
        Some(trusted_comment.as_str()),
        Some("signature from ALCOMD3 updater key"),
    )
    .context("failed to sign file")?;

    Ok(signature_box)
}

fn unix_timestamp() -> u64 {
    let start = std::time::SystemTime::now();
    let since_the_epoch = start
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is incorrect");
    since_the_epoch.as_secs()
}

#[cfg(test)]
mod tests {
    use super::{ensure_updater_env_value, normalized_secret_value, updater_env};

    #[test]
    fn secret_normalization_strips_only_a_leading_bom_and_line_endings() {
        assert_eq!(normalized_secret_value("\u{feff}c2VjcmV0\r\n"), "c2VjcmV0");
        assert_eq!(normalized_secret_value(" c2VjcmV0 "), " c2VjcmV0 ");
    }

    #[test]
    fn missing_updater_secret_is_reported_by_name() {
        const NAME: &str = "ALCOMD3_TEST_UPDATER_SECRET_THAT_MUST_NOT_EXIST";
        let error = updater_env(NAME).unwrap_err();

        assert!(error.to_string().contains(NAME));
        assert!(error.to_string().contains("Required environment variable"));
    }

    #[test]
    fn empty_updater_secret_is_reported_before_key_parsing() {
        const NAME: &str = "ALCOMD3_UPDATER_PRIVATE_KEY";
        for value in ["", "\u{feff}\r\n"] {
            let error = ensure_updater_env_value(NAME, value).unwrap_err();
            let message = error.to_string();

            assert!(message.contains(NAME));
            assert!(message.contains("is empty"));
        }
    }
}
