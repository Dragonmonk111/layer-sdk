use crate::error::PulsarResult;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::BlockInfo;

use pulsar_std::{AccountId, GasMeter, Msg, PubKey, Tx, TxError};
use pulsar_storage::{prefixed, Map, Storage};

use crate::sm::StateMachine;

pub const NAMESPACE_AUTH: &[u8] = b"auth";
const ACCOUNTS: Map<&AccountId, Account> = Map::new("accounts");

// Store magic accounts here
// TODO: Initialize with InternalAccount on startup
// TODO: make this some config?
pub fn fee_collector_account() -> AccountId {
    AccountId::new(&[7u8; 20]).unwrap()
}

#[cw_serde]
pub enum Account {
    /// This is External Account in Ethereum terms, controlled by a public key
    External { pubkey: PubKey, sequence: u64 },
    /// No pubkey can control this, either contract or "module account"
    Internal {},
    /// Used for account abstraction, where a contract can validate what a pubkey can do
    Smart {
        // FIXME: any more info to add here?
        contract: AccountId,
    },
}

pub struct Auth {
    // TODO
}

impl Auth {
    pub fn new() -> Self {
        Auth {}
    }

    /// This checks (and bumps) sequences of the local storage and deducts the fees from the account.
    /// If successful, it returns the TxData with all info that needs to be executed.
    pub fn validate_tx(
        &self,
        storage: &mut dyn Storage,
        meter: &mut GasMeter,
        _block: &BlockInfo,
        sm: &StateMachine,
        tx: Tx,
    ) -> PulsarResult<TxData> {
        // later handle other types
        let Tx::Signed(tx) = tx;

        // load the signer account if any
        let mut auth_store = prefixed(storage, NAMESPACE_AUTH);
        let pubkey = match ACCOUNTS.may_load(&auth_store, meter, &tx.signer)? {
            Some(Account::External {
                pubkey,
                mut sequence,
            }) => {
                // ensure sequence is next in line
                sequence += 1;
                if sequence != tx.signing_info.sequence {
                    return Err(TxError::InvalidSequence {
                        provided: tx.signing_info.sequence,
                        expected: sequence,
                    }
                    .into());
                }
                // if pubkey is in signing info, it must match
                if let Some(pk) = &tx.signing_info.pubkey {
                    if pk != &pubkey {
                        return Err(TxError::PubKeyMismatch {}.into());
                    }
                }
                // store the bumped sequence
                let account = Account::External {
                    pubkey: pubkey.clone(),
                    sequence,
                };
                ACCOUNTS.save(&mut auth_store, meter, &tx.signer, &account)?;
                // use the pubkey in the account to validate
                pubkey
            }
            None => {
                // if no account, ensure sequence is 1
                if tx.signing_info.sequence != 1 {
                    return Err(TxError::InvalidSequence {
                        provided: tx.signing_info.sequence,
                        expected: 1,
                    }
                    .into());
                }
                // ensure pubkey exists and matches account
                match &tx.signing_info.pubkey {
                    Some(pk) => {
                        if pk.account_id()? != tx.signer {
                            return Err(TxError::PubKeyMismatch.into());
                        }
                        let account = Account::External {
                            pubkey: pk.clone(),
                            sequence: 1,
                        };
                        ACCOUNTS.save(&mut auth_store, meter, &tx.signer, &account)?;
                        // use the pubkey in the account to validate
                        pk.clone()
                    }
                    None => return Err(TxError::PubKeyMissing.into()),
                }
            }
            Some(Account::Internal {}) => {
                // this is always prohibited
                return Err(TxError::InternalAcccount.into());
            }
            Some(Account::Smart { .. }) => {
                todo!();
            }
        };

        // validate the signature with that account (Cosmos-specific)
        pubkey.validate_signature(&tx.signing_info.message_hash, &tx.signing_info.signature)?;

        // TODO: filter logic on gas pricing.... charge min fee

        // charge fee info from tx sender (not pubkey if smart account)
        if let Some(fee) = tx.fee.fee {
            sm.bank.transfer(
                storage,
                meter,
                tx.signer.clone(),
                fee_collector_account(),
                vec![fee],
            )?;
        }

        // Return data
        Ok(TxData {
            signer: tx.signer,
            msgs: tx.msgs,
            // TODO: make some max gas limit
            gas_wanted: tx.fee.gas_limit,
        })
    }
}

impl Default for Auth {
    fn default() -> Self {
        Self::new()
    }
}

// info on a validated transaction
pub struct TxData {
    /// We only support one sender per transaction
    pub signer: AccountId,

    /// All messages in the payload, decoded to Pulsarium format
    pub msgs: Vec<Msg>,

    /// Amount of gas this transaction may use
    pub gas_wanted: u64,
}
