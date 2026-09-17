-- Seed demo API keys for local development / manual testing.
--
-- Migration 001 seeds two merchants but no api_keys, so there was no
-- credential to actually call the API with. These match the example keys
-- already used throughout README.md and documentation/api/... — generated
-- with `cargo run --bin gen_api_key -- <plaintext>` (see src/bin/gen_api_key.rs).
--
-- Plaintext keys (POC/dev only — never commit real production secrets):
--   sk_live_demo_key_001 -> Demo Merchant (standard permissions)
--   sk_live_ops_key_001  -> Operations    (operations permissions)

INSERT INTO api_keys (merchant_id, key_hash, key_prefix, label, permissions)
VALUES (
    '00000000-0000-0000-0000-000000000001',
    '$argon2id$v=19$m=19456,t=2,p=1$FldZxEyO58PRStXvZ+e83Q$IhgeKHffO/0erlredTlvbxPVch2NjeQ8RwG3KaCxEqc',
    'sk_live_de',
    'Demo standard key (POC)',
    'standard'
);

INSERT INTO api_keys (merchant_id, key_hash, key_prefix, label, permissions)
VALUES (
    '00000000-0000-0000-0000-000000000002',
    '$argon2id$v=19$m=19456,t=2,p=1$XgwCLSiK8RSIQhd4Q5Swsw$9fAIVPOtdGbSVHVRhwK4+3Nfu8E3292BetxCQr6Equ0',
    'sk_live_op',
    'Operations key (POC)',
    'operations'
);
