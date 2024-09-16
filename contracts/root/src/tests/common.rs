use cw_orch::environment::CwEnv;
use cw_orch::prelude::*;

use lch_orch::{Addressable, AltSigner};

use crate::interface::Contract;
use crate::msg::InstantiateMsg;

pub const BECH_PREFIX: &str = "slay3r";

pub fn setup<Chain: CwEnv>(chain: Chain, msg: InstantiateMsg) -> Contract<Chain> {
    let contract = Contract::new(chain);
    contract.upload().unwrap();
    contract.instantiate(&msg, None, None).unwrap();
    contract
}

/// Some basic tests to show it works.
/// Doesn't test any real logic except for one instantiation
pub fn happy_path<C>(chain: C)
where
    C: CwEnv + AltSigner,
    C::Sender: Addressable,
{
    // FIXME: put real message here
    let msg = InstantiateMsg {};
    let contract = setup(chain.clone(), msg);

    // Now we get the code id and contract address
    let code_id = contract.code_id().unwrap();
    // First on this chain
    assert_eq!(code_id, 1);
    let contract_addr = contract.address().unwrap();
    // Valid address
    assert!(contract_addr.as_str().starts_with(BECH_PREFIX));

    // show how alt accounts can be created
    let sender = chain.sender();
    assert!(sender.as_str().starts_with(BECH_PREFIX));

    // alt_signer returns a signer that can build a new daemon
    let recipient = chain.alt_signer(1);
    // addr converts the signer to an Addr
    assert!(recipient.addr().to_string().starts_with(BECH_PREFIX));
    assert_ne!(sender, recipient.addr());

    // Show how to create a new daemon with the alt signer (if recipient wants to send later)
    let alt_chain = chain.call_as(&recipient);
    assert_eq!(alt_chain.sender(), recipient.addr());
}
