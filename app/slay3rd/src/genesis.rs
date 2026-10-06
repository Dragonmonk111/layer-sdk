//! Genesis app state for a node that starts with no stored state.
//!
//! A real network initializes from `NodeConfig::genesis_path` (the output of
//! `snapshot/build-genesis.mjs`). The built-in genesis funds the devnet
//! deployer, whose private key is `SHA256("junoclaw-deployer-v1")` — public
//! to anyone who reads `tools/tx-sender` — so it is only available behind
//! `NodeConfig::insecure_devnet`.

use layer_app::genesis::{BankAccount, GenesisState, WasmParams};
use sha2::{Digest, Sha256};

use crate::config::NodeConfig;

/// Seed of the devnet deployer key (`tools/tx-sender` signs with it).
const DEVNET_DEPLOYER_SEED: &[u8] = b"junoclaw-deployer-v1";

/// `wasm.gov_account` of the built-in genesis.
const DEVNET_GOV_ACCOUNT: &str = "juno1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmdyychx";

/// Bech32 address of the devnet deployer key.
pub fn devnet_deployer_address() -> String {
    let seed = Sha256::digest(DEVNET_DEPLOYER_SEED);
    cosmrs::crypto::secp256k1::SigningKey::from_slice(&seed[..32])
        .expect("valid secp256k1 key from SHA256 seed")
        .public_key()
        .account_id(layer_std::BECH32_PREFIX)
        .expect("valid bech32 address")
        .to_string()
}

/// Default genesis state for devnet bootstrapping.
///
/// Includes a pre-funded deployer account for the tx-sender tool.
/// The deployer's secp256k1 private key is derived deterministically:
///   SHA256("junoclaw-deployer-v1")[0..32] (32 bytes)
/// The corresponding bech32 address is computed from the public key.
///
/// The tx-sender tool uses the same derivation to sign transactions.
pub fn default_genesis() -> GenesisState {
    GenesisState {
        bank: vec![BankAccount {
            address: devnet_deployer_address(),
            balance: vec![cosmwasm_std::coin(1_000_000_000_000, "ujclaw")],
        }],
        wasm: WasmParams {
            gov_account: DEVNET_GOV_ACCOUNT.to_string(),
        },
    }
}

/// Resolves the genesis app state for a fresh node.
///
/// - `genesis_path` set: parse that file. Unless `insecure_devnet` is set,
///   it may not fund, or hand `wasm.gov_account` to, a built-in devnet
///   address.
/// - `genesis_path` empty: the built-in devnet genesis, only with
///   `insecure_devnet`.
pub fn resolve_genesis(config: &NodeConfig) -> Result<GenesisState, String> {
    let path = config.genesis_path.trim();
    if path.is_empty() {
        if !config.insecure_devnet {
            return Err("genesis_path is empty. Set it to the network's genesis file; \
                the built-in genesis funds a key derived from a public constant \
                (devnets may opt in with insecure_devnet = true)"
                .to_string());
        }
        return Ok(default_genesis());
    }

    let data =
        std::fs::read(path).map_err(|e| format!("cannot read genesis file {path}: {e}"))?;
    let genesis =
        GenesisState::parse(&data).map_err(|e| format!("invalid genesis file {path}: {e}"))?;

    if !config.insecure_devnet {
        let devnet = [devnet_deployer_address(), DEVNET_GOV_ACCOUNT.to_string()];
        let exposed = genesis
            .bank
            .iter()
            .map(|account| account.address.as_str())
            .chain(std::iter::once(genesis.wasm.gov_account.as_str()))
            .find(|addr| devnet.iter().any(|d| d == addr));
        if let Some(addr) = exposed {
            return Err(format!(
                "genesis file {path} funds or empowers {addr}, a built-in devnet address \
                 (the deployer key is SHA256 of a public string). Use the ceremony key \
                 (docs/GOVERNANCE_PLAN.md)"
            ));
        }
    }

    tracing::info!(
        path,
        sha256 = %hex::encode(Sha256::digest(&data)),
        "genesis file loaded — every validator must load the identical file"
    );
    Ok(genesis)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: &str = "juno1fe2ny2twdfvt3htglnxqxejkuzt6y4wu0pg4l9";

    fn config(genesis_path: &str, insecure_devnet: bool) -> NodeConfig {
        NodeConfig {
            genesis_path: genesis_path.to_string(),
            insecure_devnet,
            ..NodeConfig::default()
        }
    }

    fn write_genesis(name: &str, funded: &str, gov: &str) -> String {
        let path = std::env::temp_dir().join(format!(
            "slay3rd-genesis-{}-{name}.json",
            std::process::id()
        ));
        let json = format!(
            r#"{{"bank":[{{"address":"{funded}","balance":[{{"denom":"ujclaw","amount":"54660000000000"}}]}}],"wasm":{{"gov_account":"{gov}"}}}}"#
        );
        std::fs::write(&path, json).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn deployer_address_matches_tx_sender_derivation() {
        assert_eq!(
            devnet_deployer_address(),
            "juno1dz875zg8p78anpjv3f0qt4gu5a3awpjfhtw992"
        );
    }

    #[test]
    fn empty_genesis_path_is_refused_without_insecure_devnet() {
        let err = resolve_genesis(&config("", false)).unwrap_err();
        assert!(err.contains("genesis_path is empty"), "{err}");
    }

    #[test]
    fn empty_genesis_path_uses_builtin_genesis_on_devnet() {
        let genesis = resolve_genesis(&config("  ", true)).unwrap();
        assert_eq!(genesis, default_genesis());
        assert_eq!(genesis.bank[0].address, devnet_deployer_address());
    }

    #[test]
    fn genesis_file_is_loaded() {
        let path = write_genesis("ok", OWNER, OWNER);
        let genesis = resolve_genesis(&config(&path, false)).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(genesis.bank.len(), 1);
        assert_eq!(genesis.bank[0].address, OWNER);
        assert_eq!(genesis.wasm.gov_account, OWNER);
    }

    #[test]
    fn genesis_file_funding_the_devnet_deployer_is_refused() {
        let path = write_genesis("deployer-funded", &devnet_deployer_address(), OWNER);
        let refused = resolve_genesis(&config(&path, false));
        let allowed = resolve_genesis(&config(&path, true));
        std::fs::remove_file(&path).unwrap();
        let err = refused.unwrap_err();
        assert!(err.contains("built-in devnet address"), "{err}");
        assert!(allowed.is_ok());
    }

    #[test]
    fn genesis_file_with_devnet_gov_account_is_refused() {
        let path = write_genesis("devnet-gov", OWNER, DEVNET_GOV_ACCOUNT);
        let res = resolve_genesis(&config(&path, false));
        std::fs::remove_file(&path).unwrap();
        assert!(res.unwrap_err().contains(DEVNET_GOV_ACCOUNT));
    }

    #[test]
    fn missing_or_malformed_genesis_file_is_an_error() {
        let missing = std::env::temp_dir().join("slay3rd-genesis-does-not-exist.json");
        let err = resolve_genesis(&config(&missing.to_string_lossy(), true)).unwrap_err();
        assert!(err.contains("cannot read genesis file"), "{err}");

        let path = std::env::temp_dir().join(format!(
            "slay3rd-genesis-{}-malformed.json",
            std::process::id()
        ));
        std::fs::write(&path, b"{\"bank\": 5}").unwrap();
        let res = resolve_genesis(&config(&path.to_string_lossy(), true));
        std::fs::remove_file(&path).unwrap();
        assert!(res.unwrap_err().contains("invalid genesis file"));
    }
}
