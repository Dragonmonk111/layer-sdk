import { coins, DirectSecp256k1HdWallet } from "@cosmjs/proto-signing";
import {
  assertIsDeliverTxFailure,
  assertIsDeliverTxSuccess,
  MsgSendEncodeObject,
  SigningStargateClient,
} from "@cosmjs/stargate";
import { Tendermint37Client } from "@cosmjs/tendermint-rpc";
import { MsgSend } from "cosmjs-types/cosmos/bank/v1beta1/tx";

import {
  defaultGasPrice,
  defaultSendFee,
  defaultSigningClientOptions,
  defaultWalletOptions,
  DENOM,
  faucet,
  makeRandomAddress,
  pulsarium,
} from "./testutils.spec";

describe("SigningStargateClient", () => {
  describe("simulate", () => {
    it("works", async () => {
      const wallet = await DirectSecp256k1HdWallet.fromMnemonic(faucet.mnemonic, defaultWalletOptions);
      const tendermintClient = await Tendermint37Client.connect(pulsarium.tendermintUrl);
      const client = await SigningStargateClient.createWithSigner(
        tendermintClient,
        wallet,
        defaultSigningClientOptions
      );

      const msg = MsgSend.fromPartial({
        fromAddress: faucet.address0,
        toAddress: makeRandomAddress(),
        amount: coins(2000000, DENOM),
      });
      const msgAny: MsgSendEncodeObject = {
        typeUrl: "/cosmos.bank.v1beta1.MsgSend",
        value: msg,
      };
      const memo = "Use your power wisely";
      const gasUsed = await client.simulate(faucet.address0, [msgAny], memo);
      // TODO: more realistic gas estimate (something not measured here)
      expect(gasUsed).toBeGreaterThanOrEqual(3_000);
      expect(gasUsed).toBeLessThanOrEqual(60_000);

      client.disconnect();
    });
  });

  describe("sendTokens", () => {
    it("works with direct signer", async () => {
      const wallet = await DirectSecp256k1HdWallet.fromMnemonic(faucet.mnemonic, defaultWalletOptions);
      const tendermintClient = await Tendermint37Client.connect(pulsarium.tendermintUrl);
      const client = await SigningStargateClient.createWithSigner(
        tendermintClient,
        wallet,
        defaultSigningClientOptions
      );

      const amount = coins(7890, DENOM);
      const beneficiaryAddress = makeRandomAddress();
      const memo = "for dinner";

      // no tokens here
      const before = await client.getBalance(beneficiaryAddress, DENOM);
      expect(before).toEqual({
        denom: DENOM,
        amount: "0",
      });

      // send
      const result = await client.sendTokens(faucet.address0, beneficiaryAddress, amount, defaultSendFee, memo);
      assertIsDeliverTxSuccess(result);
      expect(result.rawLog).toBeTruthy();

      // got tokens
      const after = await client.getBalance(beneficiaryAddress, DENOM);
      expect(after).toEqual(amount[0]);
    });

    xit("works with legacy Amino signer", async () => {
      const wallet = await DirectSecp256k1HdWallet.fromMnemonic(faucet.mnemonic, defaultWalletOptions);
      const tendermintClient = await Tendermint37Client.connect(pulsarium.tendermintUrl);
      const client = await SigningStargateClient.createWithSigner(
        tendermintClient,
        wallet,
        defaultSigningClientOptions
      );

      const amount = coins(7890, DENOM);
      const beneficiaryAddress = makeRandomAddress();
      const memo = "for dinner";

      // no tokens here
      const before = await client.getBalance(beneficiaryAddress, DENOM);
      expect(before).toEqual({
        denom: DENOM,
        amount: "0",
      });

      // send
      const result = await client.sendTokens(faucet.address0, beneficiaryAddress, amount, defaultSendFee, memo);
      assertIsDeliverTxSuccess(result);
      expect(result.rawLog).toBeTruthy();

      // got tokens
      const after = await client.getBalance(beneficiaryAddress, DENOM);
      expect(after).toEqual(amount[0]);
    });
  });

  it("returns DeliverTxFailure on DeliverTx failure", async () => {
    const wallet = await DirectSecp256k1HdWallet.fromMnemonic(faucet.mnemonic, defaultWalletOptions);
    const tendermintClient = await Tendermint37Client.connect(pulsarium.tendermintUrl);
    const client = await SigningStargateClient.createWithSigner(tendermintClient, wallet, defaultSigningClientOptions);

    const msg = MsgSend.fromPartial({
      fromAddress: faucet.address0,
      toAddress: makeRandomAddress(),
      amount: coins(Number.MAX_SAFE_INTEGER, DENOM),
    });
    const msgAny: MsgSendEncodeObject = {
      typeUrl: "/cosmos.bank.v1beta1.MsgSend",
      value: msg,
    };
    const fee = {
      amount: coins(2000, DENOM),
      gas: "99000",
    };

    // Only auth check in CheckTx gives this:
    const result = await client.signAndBroadcast(faucet.address0, [msgAny], fee);
    assertIsDeliverTxFailure(result);
    expect(result.code).toBeGreaterThan(0);
    expect(result.rawLog).toMatch(/insufficient funds/);
  });

  it("works with auto gas", async () => {
    const wallet = await DirectSecp256k1HdWallet.fromMnemonic(faucet.mnemonic, defaultWalletOptions);
    const tendermintClient = await Tendermint37Client.connect(pulsarium.tendermintUrl);
    const client = await SigningStargateClient.createWithSigner(tendermintClient, wallet, {
      ...defaultSigningClientOptions,
      gasPrice: defaultGasPrice,
    });

    const msg = MsgSend.fromPartial({
      fromAddress: faucet.address0,
      toAddress: makeRandomAddress(),
      amount: coins(2000000, DENOM),
    });
    const msgAny: MsgSendEncodeObject = {
      typeUrl: "/cosmos.bank.v1beta1.MsgSend",
      value: msg,
    };
    // TODO: clairfy simulate gas for lmdb
    // const result = await client.signAndBroadcast(faucet.address0, [msgAny], "auto");
    const result = await client.signAndBroadcast(faucet.address0, [msgAny], 3);
    assertIsDeliverTxSuccess(result);
  });
});
