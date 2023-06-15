import { SigningCosmWasmClient } from "@cosmjs/cosmwasm-stargate";
import { DirectSecp256k1HdWallet } from "@cosmjs/proto-signing";
import { calculateFee } from "@cosmjs/stargate";
import { Tendermint37Client } from "@cosmjs/tendermint-rpc";
import fs from "fs";

import {
  defaultGasPrice,
  defaultSigningClientOptions,
  defaultWalletOptions,
  faucet,
  makeRandomAddress,
  pulsarium,
} from "./testutils.spec";

describe("Cw20 Test Cases", () => {
  describe("happyPath", () => {
    fit("works with direct signer", async () => {
      const signer = faucet.address0;
      const wallet = await DirectSecp256k1HdWallet.fromMnemonic(faucet.mnemonic, defaultWalletOptions);
      const tendermintClient = await Tendermint37Client.connect(pulsarium.tendermintUrl);
      const client = await SigningCosmWasmClient.createWithSigner(
        tendermintClient,
        wallet,
        defaultSigningClientOptions
      );

      // store code
      const wasmBuf = fs.readFileSync(__dirname + "/../../packages/app/fixtures/cw20_base.wasm");
      const wasm = new Uint8Array(wasmBuf); // not sure if this is needed but something is odd here (always out of gas)
      console.info(`Wasm size: ${wasm.length} bytes`);
      const uploadFee = calculateFee(50_000_000, defaultGasPrice);
      console.info(`Upload fee: ${JSON.stringify(uploadFee)}`);

      const uploadReceipt = await client.upload(signer, wasm, uploadFee);
      console.info(`Upload succeeded. Receipt: ${JSON.stringify(uploadReceipt)}`);
      expect(uploadReceipt).toBeTruthy();
      const { codeId } = uploadReceipt;
      expect(codeId).toBeTruthy();

      // instantiate contract
      const initMsg = {
        name: "pulsar",
        symbol: "PULSE",
        decimals: 6,
        initial_balances: [
          {
            address: signer,
            amount: "50000000", // 50 PULSE
          },
        ],
      };

      const instantiateFee = calculateFee(500_000, defaultGasPrice);
      const { contractAddress } = await client.instantiate(signer, codeId, initMsg, "PULSE Token", instantiateFee, {
        memo: `Create a hackatom instance in deploy_hackatom.js`,
        admin: signer,
      });
      expect(contractAddress).toBeTruthy();

      // query balances
      const recipient = makeRandomAddress();
      const myBal = await client.queryContractSmart(contractAddress, { balance: { address: faucet.address0 } });
      console.info(`My balance: ${JSON.stringify(myBal)}`);
      expect(myBal.balance).toEqual("50000000");
      const yourBal = await client.queryContractSmart(contractAddress, { balance: { address: recipient } });
      console.info(`Your balance: ${JSON.stringify(yourBal)}`);
      expect(yourBal.balance).toEqual("0");
    });

    //   describe("simulate", () => {
    //     it("works", async () => {
    //       const wallet = await DirectSecp256k1HdWallet.fromMnemonic(faucet.mnemonic, defaultWalletOptions);
    //       const tendermintClient = await Tendermint37Client.connect(pulsarium.tendermintUrl);
    //       const client = await SigningStargateClient.createWithSigner(
    //         tendermintClient,
    //         wallet,
    //         defaultSigningClientOptions
    //       );

    //       const msg = MsgSend.fromPartial({
    //         fromAddress: faucet.address0,
    //         toAddress: makeRandomAddress(),
    //         amount: coins(2000000, DENOM),
    //       });
    //       const msgAny: MsgSendEncodeObject = {
    //         typeUrl: "/cosmos.bank.v1beta1.MsgSend",
    //         value: msg,
    //       };
    //       const memo = "Use your power wisely";
    //       const gasUsed = await client.simulate(faucet.address0, [msgAny], memo);
    //       // TODO: more realistic gas estimate (something not measured here)
    //       expect(gasUsed).toBeGreaterThanOrEqual(3_000);
    //       expect(gasUsed).toBeLessThanOrEqual(60_000);

    //       client.disconnect();
    //     });
    //   });
  });
});
