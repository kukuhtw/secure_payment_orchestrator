//! Dev/ops tool: generates an API key ready to insert into `api_keys`.
//!
//! The migration only seeds `merchants`, not `api_keys` (see
//! `documentation/reports/Progress-Report.md`), so there was no supported
//! way to create merchant credentials for local testing or production
//! onboarding. This tool prints the plaintext key (shown once, hand it to
//! the merchant), its lookup prefix, and its Argon2 hash — everything
//! needed for an `INSERT INTO api_keys (...)`.
//!
//! Usage:
//!   cargo run --bin gen_api_key [-- <plaintext_key>]
//!
//! If no plaintext key is given, a random `sk_live_<uuid>` one is generated.

use spo_api::security::api_key::key_prefix;
use spo_api::security::hash::hash_secret;
use uuid::Uuid;

fn main() -> anyhow::Result<()> {
    let plain = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("sk_live_{}", Uuid::new_v4().simple()));

    let prefix = key_prefix(&plain);
    let hash = hash_secret(&plain).map_err(|e| anyhow::anyhow!("failed to hash API key: {e}"))?;

    println!("Full API key (show once — hand this to the merchant): {plain}");
    println!("key_prefix (store as-is, indexed lookup column):      {prefix}");
    println!("key_hash   (store as-is, Argon2id PHC string):        {hash}");
    println!();
    println!("-- Example insert (adjust merchant_id, label, permissions):");
    println!(
        "INSERT INTO api_keys (merchant_id, key_hash, key_prefix, label, permissions)\nVALUES ('<merchant_id>', '{hash}', '{prefix}', '<label>', 'standard');"
    );

    Ok(())
}
