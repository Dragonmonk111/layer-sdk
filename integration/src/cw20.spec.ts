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
    it("works with direct signer", async () => {
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

      const uploadReceipt = await client.upload(signer, wasm, uploadFee);
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

      const instantiateFee = calculateFee(300_000, defaultGasPrice);
      const { contractAddress } = await client.instantiate(signer, codeId, initMsg, "PULSE Token", instantiateFee, {
        memo: `Create a hackatom instance in deploy_hackatom.js`,
        admin: signer,
      });
      expect(contractAddress).toBeTruthy();

      // query balances
      const recipient = makeRandomAddress();
      const myBal = await client.queryContractSmart(contractAddress, { balance: { address: faucet.address0 } });
      expect(myBal.balance).toEqual("50000000");
      const yourBal = await client.queryContractSmart(contractAddress, { balance: { address: recipient } });
      expect(yourBal.balance).toEqual("0");

      const executeFee = calculateFee(300_000, defaultGasPrice);
      const execMsg = {
        transfer: {
          recipient: recipient,
          amount: "42000000",
        },
      };
      await client.execute(signer, contractAddress, execMsg, executeFee);

      // query balances
      const myBal2 = await client.queryContractSmart(contractAddress, { balance: { address: faucet.address0 } });
      expect(myBal2.balance).toEqual("8000000");
      const yourBal2 = await client.queryContractSmart(contractAddress, { balance: { address: recipient } });
      expect(yourBal2.balance).toEqual("42000000");
    });

    // this ensures the simulate calls work for all of the messages
    it("works with gas price simulation", async () => {
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

      const uploadReceipt = await client.upload(signer, wasm, "auto");
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

      const { contractAddress } = await client.instantiate(signer, codeId, initMsg, "PULSE Token", "auto", {
        memo: `Create a hackatom instance in deploy_hackatom.js`,
        admin: signer,
      });
      expect(contractAddress).toBeTruthy();

      // query balances
      const recipient = makeRandomAddress();
      const myBal = await client.queryContractSmart(contractAddress, { balance: { address: faucet.address0 } });
      expect(myBal.balance).toEqual("50000000");
      const yourBal = await client.queryContractSmart(contractAddress, { balance: { address: recipient } });
      expect(yourBal.balance).toEqual("0");

      const execMsg = {
        transfer: {
          recipient: recipient,
          amount: "42000000",
        },
      };
      await client.execute(signer, contractAddress, execMsg, "auto");

      // query balances
      const myBal2 = await client.queryContractSmart(contractAddress, { balance: { address: faucet.address0 } });
      expect(myBal2.balance).toEqual("8000000");
      const yourBal2 = await client.queryContractSmart(contractAddress, { balance: { address: recipient } });
      expect(yourBal2.balance).toEqual("42000000");
    });
  });
});
