use anyhow::{Context, Result};
use base64::Engine;
use minisign::KeyPair;
use std::fs;
use std::path::PathBuf;

/// Generates a minisign key pair for ALCOMD3 updater payloads.
#[derive(clap::Parser)]
pub struct Command {
    #[arg(long, default_value = "artifacts/alcomd3-updater-key")]
    out_dir: PathBuf,
    #[arg(long, env = "ALCOMD3_UPDATER_PRIVATE_KEY_PASSWORD")]
    password: String,
}

impl crate::Command for Command {
    fn run(self) -> Result<i32> {
        fs::create_dir_all(&self.out_dir).with_context(|| {
            format!(
                "failed to create output directory: {}",
                self.out_dir.display()
            )
        })?;

        let key_pair = KeyPair::generate_encrypted_keypair(Some(self.password.clone()))
            .context("failed to generate updater key pair")?;
        let public_key_box = key_pair
            .pk
            .to_box()
            .context("failed to encode public key")?;
        let secret_key_box = key_pair
            .sk
            .to_box(Some("ALCOMD3 updater secret key"))
            .context("failed to encode secret key")?;

        let public_key =
            base64::engine::general_purpose::STANDARD.encode(public_key_box.to_string());
        let private_key =
            base64::engine::general_purpose::STANDARD.encode(secret_key_box.to_string());

        fs::write(self.out_dir.join("public-key-base64.txt"), &public_key)
            .context("failed to write public key")?;
        fs::write(
            self.out_dir.join("private-key.env"),
            format!(
                "ALCOMD3_UPDATER_PRIVATE_KEY={private_key}\nALCOMD3_UPDATER_PRIVATE_KEY_PASSWORD={}\n",
                self.password
            ),
        )
        .context("failed to write private key env file")?;
        fs::write(
            self.out_dir.join("private-key.ps1"),
            format!(
                "$env:ALCOMD3_UPDATER_PRIVATE_KEY={}\n$env:ALCOMD3_UPDATER_PRIVATE_KEY_PASSWORD={}\n",
                powershell_string(&private_key),
                powershell_string(&self.password)
            ),
        )
        .context("failed to write PowerShell private key env file")?;

        println!("Generated updater key pair.");
        println!("Public key base64: {public_key}");
        println!(
            "Private key env file: {}",
            self.out_dir.join("private-key.env").display()
        );
        println!(
            "PowerShell private key env file: {}",
            self.out_dir.join("private-key.ps1").display()
        );
        println!(
            "Public key file: {}",
            self.out_dir.join("public-key-base64.txt").display()
        );

        Ok(0)
    }
}

fn powershell_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
