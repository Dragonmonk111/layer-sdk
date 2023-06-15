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
      const wallet = await DirectSecp256k1HdWallet.fromMnemonic(faucet.mnemonic, defaultWalletOptions);
      const tendermintClient = await Tendermint37Client.connect(pulsarium.tendermintUrl);
      const client = await SigningCosmWasmClient.createWithSigner(
        tendermintClient,
        wallet,
        defaultSigningClientOptions
      );

      // store code
      const wasm = fs.readFileSync(__dirname + "/../../packages/app/fixtures/cw20_base.wasm");
      console.info(`Wasm size: ${wasm.length} bytes`);
      // const uploadFee = calculateFee(500_000_000, defaultGasPrice);
      // const uploadReceipt = await client.upload(faucet.address0, wasm, uploadFee, "Upload cw20_base");
      const uploadReceipt = await client.upload(faucet.address0, wasm, 50_000_000, "Upload cw20_base");
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
            address: faucet.address0,
            amount: "50000000", // 50 PULSE
          },
        ],
      };

      const instantiateFee = calculateFee(500_000, defaultGasPrice);
      const { contractAddress } = await client.instantiate(
        faucet.address0,
        codeId,
        initMsg,
        "PULSE Token",
        instantiateFee,
        {
          memo: `Create a hackatom instance in deploy_hackatom.js`,
          admin: faucet.address0,
        }
      );
      expect(contractAddress).toBeTruthy();

      // query balances
      const recipient = makeRandomAddress();
      const myBal = await client.queryContractSmart(contractAddress, { balance: { address: faucet.address0 } });
      console.info(`My balance: ${JSON.stringify(myBal)}`);
      expect(myBal.amount).toEqual(50_000_000);
      const yourBal = await client.queryContractSmart(contractAddress, { balance: { address: recipient } });
      console.info(`Your balance: ${JSON.stringify(yourBal)}`);
      expect(yourBal.amount).toEqual(0);
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
