import { Secp256k1HdWallet } from "@cosmjs/amino";
import { SigningCosmWasmClient } from "@cosmjs/cosmwasm-stargate";
import { setupWasmExtension } from "@cosmjs/cosmwasm-stargate";
import { DirectSecp256k1HdWallet } from "@cosmjs/proto-signing";
import { calculateFee } from "@cosmjs/stargate";
import { QueryClient, setupAuthExtension, setupBankExtension } from "@cosmjs/stargate";
import { Comet38Client } from "@cosmjs/tendermint-rpc";
import fs from "fs";

import {
  defaultGasPrice,
  defaultSigningClientOptions,
  defaultWalletOptions,
  faucet,
  makeRandomAddress,
  localNet,
} from "./testutils.spec";

describe("Upload Todo List", () => {
  describe("happyPath", () => {
    it("uploads", async () => {
      const signer = faucet.address0;
      const wallet = await DirectSecp256k1HdWallet.fromMnemonic(faucet.mnemonic, defaultWalletOptions);
      const cometClient = await Comet38Client.connect(localNet.tendermintUrl);
      const client = await SigningCosmWasmClient.createWithSigner(
        cometClient,
        wallet,
        defaultSigningClientOptions
      );

      // store code
      const wasmBuf = fs.readFileSync(__dirname + "/../testdata/todo_list.wasm");
      const wasm = new Uint8Array(wasmBuf); // not sure if this is needed but something is odd here (always out of gas)
      console.info(`Wasm size: ${wasm.length} bytes`);
      const uploadReceipt = await client.upload(signer, wasm, "auto");
      expect(uploadReceipt).toBeTruthy();
      const { codeId } = uploadReceipt;
      expect(codeId).toBeTruthy();
      console.info(`Todo List code ID: ${codeId}`);
    });
  });
});

describe("Cw20 Test Cases", () => {
  describe("happyPath", () => {
    it("works with direct signer", async () => {
      const signer = faucet.address0;
      const wallet = await DirectSecp256k1HdWallet.fromMnemonic(faucet.mnemonic, defaultWalletOptions);
      const cometClient = await Comet38Client.connect(localNet.tendermintUrl);
      const client = await SigningCosmWasmClient.createWithSigner(
        cometClient,
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
        name: "Metalheads Unite",
        symbol: "METAL",
        decimals: 6,
        initial_balances: [
          {
            address: signer,
            amount: "50000000", // 50 METAL
          },
        ],
      };

      const instantiateFee = calculateFee(300_000, defaultGasPrice);
      const { contractAddress } = await client.instantiate(signer, codeId, initMsg, "METAL Token", instantiateFee, {
        memo: `Create a cw20 instance`,
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
      const cometClient = await Comet38Client.connect(localNet.tendermintUrl);
      const client = await SigningCosmWasmClient.createWithSigner(
        cometClient,
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
        name: "Metalheads Unite",
        symbol: "METAL",
        decimals: 6,
        initial_balances: [
          {
            address: signer,
            amount: "50000000", // 50 METAL
          },
        ],
      };

      const { contractAddress } = await client.instantiate(signer, codeId, initMsg, "METAL Token", "auto", {
        memo: `Create a cw20 instance`,
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

    it("legacy signer works for execute and instantiate", async () => {
      // we need this for the upload
      const signer = faucet.address0;
      const directWallet = await DirectSecp256k1HdWallet.fromMnemonic(faucet.mnemonic, defaultWalletOptions);
      const directClient = await SigningCosmWasmClient.createWithSigner(
        await Comet38Client.connect(localNet.tendermintUrl),
        directWallet,
        defaultSigningClientOptions
      );

      // ensure this works for instantiate and execute
      const wallet = await Secp256k1HdWallet.fromMnemonic(faucet.mnemonic, defaultWalletOptions);
      const client = await SigningCosmWasmClient.createWithSigner(
        await Comet38Client.connect(localNet.tendermintUrl),
        wallet,
        defaultSigningClientOptions
      );

      // store code
      const wasmBuf = fs.readFileSync(__dirname + "/../../packages/app/fixtures/cw20_base.wasm");
      const wasm = new Uint8Array(wasmBuf); // not sure if this is needed but something is odd here (always out of gas)

      const uploadReceipt = await directClient.upload(signer, wasm, "auto");
      expect(uploadReceipt).toBeTruthy();
      const { codeId } = uploadReceipt;
      expect(codeId).toBeTruthy();

      // instantiate contract
      const initMsg = {
        name: "Metalheads Unite",
        symbol: "METAL",
        decimals: 6,
        initial_balances: [
          {
            address: signer,
            amount: "50000000", // 50 METAL
          },
        ],
      };

      const { contractAddress } = await client.instantiate(signer, codeId, initMsg, "METAL Token", "auto", {
        memo: `Create a cw20 instance`,
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

describe("Contract Migrate", () => {
  it("migrate new contract code", async () => {
    const signer = faucet.address0;
    const directWallet = await DirectSecp256k1HdWallet.fromMnemonic(faucet.mnemonic, defaultWalletOptions);
    const directClient = await SigningCosmWasmClient.createWithSigner(
      await Comet38Client.connect(localNet.tendermintUrl),
      directWallet,
      defaultSigningClientOptions
    );

    // store code
    const wasmBuf = fs.readFileSync(__dirname + "/../../packages/app/fixtures/cw20_base.wasm");
    const wasm = new Uint8Array(wasmBuf); // not sure if this is needed but something is odd here (always out of gas)

    const uploadReceipt = await directClient.upload(signer, wasm, "auto");
    expect(uploadReceipt).toBeTruthy();
    const { codeId: codeId1 } = uploadReceipt;
    expect(codeId1).toBeTruthy();

    // store another code
    const uploadReceipt2 = await directClient.upload(signer, wasm, "auto");
    expect(uploadReceipt2).toBeTruthy();
    const { codeId: codeId2 } = uploadReceipt;
    expect(codeId2).toBeTruthy();

    // instantiate contract
    const initMsg = {
      name: "Metalheads Unite",
      symbol: "METAL",
      decimals: 6,
      initial_balances: [
        {
          address: signer,
          amount: "50000000", // 50 METAL
        },
      ],
    };

    const { contractAddress } = await directClient.instantiate(signer, codeId1, initMsg, "METAL Token", "auto", {
      memo: `Create a cw20 instance`,
      admin: signer,
    });
    expect(contractAddress).toBeTruthy();

    const cmClient = await Comet38Client.connect(localNet.tendermintUrl);
    const wasmClient = QueryClient.withExtensions(cmClient, setupAuthExtension, setupBankExtension, setupWasmExtension);
    const { contractInfo: contractInfo1 } = await wasmClient.wasm.getContractInfo(contractAddress);
    console.info(`contractInfo1 admin is same as faucet address0: ${contractInfo1.admin === faucet.address0}`);
    expect(contractInfo1.admin).toEqual(signer);

    // execute migrate msg
    const newVerifier = makeRandomAddress();
    const migrateMsg = { admin: newVerifier };

    const { logs, events, height, gasUsed, gasWanted } = await directClient.migrate(
      faucet.address0,
      contractAddress,
      codeId2,
      migrateMsg,
      "auto"
    );
    console.info(`height: ${height}`);
    console.info(`gasUsed: ${gasUsed}`);
    console.info(`gasWanted: ${gasWanted}`);

    // Custom serializer for BigInt
    const customSerializer = (key: string, value: any): any => {
      return typeof value === "bigint" ? value.toString() : value;
    };

    // logs and events
    console.info(`logs: ${JSON.stringify(logs)}`);
    console.info(`events: ${JSON.stringify(events)}`);

    const { contractInfo: contractInfo2 } = await wasmClient.wasm.getContractInfo(contractAddress);
    console.info(`contractInfo2: ${JSON.stringify(contractInfo2, customSerializer)}`);
    expect(contractInfo2.admin).toEqual(newVerifier);
    console.info(
      `contractInfo2 admin is same as newVerifier: ${contractInfo2.admin === newVerifier}, ${newVerifier}, ${
        contractInfo2.admin
      }`
    );
  });
});
