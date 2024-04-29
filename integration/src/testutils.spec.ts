/* eslint-disable @typescript-eslint/naming-convention */
import { AminoSignResponse, Secp256k1HdWallet, Secp256k1HdWalletOptions, StdSignDoc } from "@cosmjs/amino";
import { Bip39, EnglishMnemonic, Random } from "@cosmjs/crypto";
import { toBech32 } from "@cosmjs/encoding";
import {
  coins,
  DirectSecp256k1HdWallet,
  DirectSecp256k1HdWalletOptions,
  DirectSignResponse,
  makeAuthInfoBytes,
} from "@cosmjs/proto-signing";
import { calculateFee, GasPrice, SigningStargateClientOptions } from "@cosmjs/stargate";
import { SignMode } from "cosmjs-types/cosmos/tx/signing/v1beta1/signing";
import { AuthInfo, SignDoc, TxBody } from "cosmjs-types/cosmos/tx/v1beta1/tx";

export const PREFIX = "slay3r";

export const DENOM = "uslay";

export function makeRandomAddressBytes(): Uint8Array {
  return Random.getBytes(20);
}

export function makeRandomAddress(): string {
  return toBech32(PREFIX, makeRandomAddressBytes());
}

/** Returns first element. Throws if array has a different length than 1. */
export function fromOneElementArray<T>(elements: ArrayLike<T>): T {
  if (elements.length !== 1) throw new Error(`Expected exactly one element but got ${elements.length}`);
  return elements[0];
}

export const defaultGasPrice = GasPrice.fromString("0.025" + DENOM);
export const defaultSendFee = calculateFee(100_000, defaultGasPrice);

export const hostName = "localhost";
// export const hostName = "65.21.105.220";

export const localNet = {
  tendermintUrl: `http://${hostName}:26657`,
  tendermintUrlWs: `ws://${hostName}:26657`,
  tendermintUrlHttp: `http://${hostName}:26657`,
  chainId: "slay3r-dev",
  denomStaking: DENOM,
  denomFee: DENOM,
  blockTime: 1_000, // ms
  totalSupply: 21000000000, // uslay
};

/** Setting to speed up testing */
export const defaultSigningClientOptions: SigningStargateClientOptions = {
  broadcastPollIntervalMs: 300,
  broadcastTimeoutMs: 8_000,
  gasPrice: defaultGasPrice,
};

export const defaultWalletOptions: Partial<DirectSecp256k1HdWalletOptions> = {
  prefix: PREFIX,
};

export const faucet = {
  mnemonic:
    "economy stock theory fatal elder harbor betray wasp final emotion task crumble siren bottom lizard educate guess current outdoor pair theory focus wife stone",
  pubkey0: {
    type: "tendermint/PubKeySecp256k1",
    value: "A08EGB7ro1ORuFhjOnZcSgwYlpe0DSFjVNUIkNNQxwKQ",
  },
  pubkey1: {
    type: "tendermint/PubKeySecp256k1",
    value: "AiDosfIbBi54XJ1QjCeApumcy/FjdtF+YhywPf3DKTx7",
  },
  pubkey2: {
    type: "tendermint/PubKeySecp256k1",
    value: "AzQg33JZqH7vSsm09esZY5bZvmzYwE/SY78cA0iLxpD7",
  },
  pubkey3: {
    type: "tendermint/PubKeySecp256k1",
    value: "A3gOAlB6aiRTCPvWMQg2+ZbGYNsLd8qlvV28m8p2UhY2",
  },
  pubkey4: {
    type: "tendermint/PubKeySecp256k1",
    value: "Aum2063ub/ErUnIUB36sK55LktGUStgcbSiaAnL1wadu",
  },
  address0: "slay3r1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmvk3r3j",
  address1: "slay3r10dyr9899g6t0pelew4nvf4j5c3jcgv0rf3kguu",
  address2: "slay3r1xy4yqngt0nlkdcenxymg8tenrghmek4nvqd2x7",
  address3: "slay3r142u9fgcjdlycfcez3lw8x6x5h7rfjlnfqpc27w",
  address4: "slay3r1hsm76p4ahyhl5yh3ve9ur49r5kemhp2rcdwjdw",
};

/** Unused account */
export const unused = {
  pubkey: {
    type: "tendermint/PubKeySecp256k1",
    value: "ArkCaFUJ/IH+vKBmNRCdUVl3mCAhbopk9jjW4Ko4OfRQ",
  },
  address: "slay3r1cjsxept9rkggzxztslae9ndgpdyt2408gk35w5",
  accountNumber: 0,
  sequence: 0,
  balanceFee: "1000000000", // 1000 PULSE
};

export const validator = {
  /**
   * From first gentx's auth_info.signer_infos in scripts/simapp44/template/.simapp/config/genesis.json
   *
   * ```
   * jq ".app_state.genutil.gen_txs[0].auth_info.signer_infos[0].public_key" scripts/simapp44/template/.simapp/config/genesis.json
   * ```
   */
  pubkey: {
    type: "tendermint/PubKeySecp256k1",
    value: "AtDcuH4cX1eaxZrJ5shheLG3tXPAoV4awoIZmNQtQxmf",
  },
  /**
   * delegator_address from /cosmos.staking.v1beta1.MsgCreateValidator in scripts/simapp44/template/.simapp/config/genesis.json
   *
   * ```
   * jq ".app_state.genutil.gen_txs[0].body.messages[0].delegator_address" scripts/simapp44/template/.simapp/config/genesis.json
   * ```
   */
  delegatorAddress: "cosmos1urk9gy7cfws0ak9x5nu7lx4un9n6gqkry79679",
  /**
   * validator_address from /cosmos.staking.v1beta1.MsgCreateValidator in scripts/simapp44/template/.simapp/config/genesis.json
   *
   * ```
   * jq ".app_state.genutil.gen_txs[0].body.messages[0].validator_address" scripts/simapp44/template/.simapp/config/genesis.json
   * ```
   */
  validatorAddress: "cosmosvaloper1urk9gy7cfws0ak9x5nu7lx4un9n6gqkrp230jk",
  accountNumber: 0,
  sequence: 1,
};

export const nonExistentAddress = "slay3r1p79apjaufyphcmsn4g07cynqf0wyjuezhu34k9";

export const nonNegativeIntegerMatcher = /^[0-9]+$/;
export const tendermintIdMatcher = /^[0-9A-F]{64}$/;

/**
 * A class for testing clients using an Amino signer which modifies the transaction it receives before signing
 */
export class ModifyingSecp256k1HdWallet extends Secp256k1HdWallet {
  public static override async fromMnemonic(
    mnemonic: string,
    options: Partial<Secp256k1HdWalletOptions> = {}
  ): Promise<ModifyingSecp256k1HdWallet> {
    const mnemonicChecked = new EnglishMnemonic(mnemonic);
    const seed = await Bip39.mnemonicToSeed(mnemonicChecked, options.bip39Password);
    return new ModifyingSecp256k1HdWallet(mnemonicChecked, { ...options, seed: seed });
  }

  public override async signAmino(signerAddress: string, signDoc: StdSignDoc): Promise<AminoSignResponse> {
    const modifiedSignDoc = {
      ...signDoc,
      fee: {
        amount: coins(3000, DENOM),
        gas: "333333",
      },
      memo: "This was modified",
    };
    return super.signAmino(signerAddress, modifiedSignDoc);
  }
}

/**
 * A class for testing clients using a direct signer which modifies the transaction it receives before signing
 */
export class ModifyingDirectSecp256k1HdWallet extends DirectSecp256k1HdWallet {
  public static override async fromMnemonic(
    mnemonic: string,
    options: Partial<DirectSecp256k1HdWalletOptions> = {}
  ): Promise<DirectSecp256k1HdWallet> {
    const mnemonicChecked = new EnglishMnemonic(mnemonic);
    const seed = await Bip39.mnemonicToSeed(mnemonicChecked, options.bip39Password);
    return new ModifyingDirectSecp256k1HdWallet(mnemonicChecked, { ...options, seed: seed });
  }

  public override async signDirect(address: string, signDoc: SignDoc): Promise<DirectSignResponse> {
    const txBody = TxBody.decode(signDoc.bodyBytes);
    const modifiedTxBody = TxBody.fromPartial({
      ...txBody,
      memo: "This was modified",
    });
    const authInfo = AuthInfo.decode(signDoc.authInfoBytes);
    const signers = authInfo.signerInfos.map((signerInfo) => ({
      pubkey: signerInfo.publicKey!,
      sequence: signerInfo.sequence.toNumber(),
    }));
    const modifiedFeeAmount = coins(3000, DENOM);
    const modifiedGasLimit = 333333;
    const modifiedFeeGranter = undefined;
    const modifiedFeePayer = undefined;
    const modifiedSignDoc = {
      ...signDoc,
      bodyBytes: Uint8Array.from(TxBody.encode(modifiedTxBody).finish()),
      authInfoBytes: makeAuthInfoBytes(
        signers,
        modifiedFeeAmount,
        modifiedGasLimit,
        modifiedFeeGranter,
        modifiedFeePayer,
        SignMode.SIGN_MODE_DIRECT
      ),
    };
    return super.signDirect(address, modifiedSignDoc);
  }
}
