import { Secp256k1HdWallet } from "@cosmjs/amino";
import { SigningCosmWasmClient } from "@cosmjs/cosmwasm-stargate";
import { DirectSecp256k1HdWallet } from "@cosmjs/proto-signing";
import { calculateFee } from "@cosmjs/stargate";
import { Comet38Client } from "@cosmjs/tendermint-rpc";
import fs from "fs";

import {
  defaultGasPrice,
  defaultSigningClientOptions,
  defaultWalletOptions,
  faucet,
  localNet,
  makeRandomAddress,
} from "./testutils.spec";

describe("Upload Todo List", () => {
  describe("happyPath", () => {
    it("uploads", async () => {
      const signer = faucet.address0;
      const wallet = await DirectSecp256k1HdWallet.fromMnemonic(faucet.mnemonic, defaultWalletOptions);
      const cometClient = await Comet38Client.connect(localNet.tendermintUrl);
      const client = await SigningCosmWasmClient.createWithSigner(cometClient, wallet, defaultSigningClientOptions);

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
      const client = await SigningCosmWasmClient.createWithSigner(cometClient, wallet, defaultSigningClientOptions);

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
      const client = await SigningCosmWasmClient.createWithSigner(cometClient, wallet, defaultSigningClientOptions);

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
  fit("migrate new contract code", async () => {
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
    const { codeId: codeId2 } = uploadReceipt2;
    expect(codeId2).toBeTruthy();
    expect(codeId1).not.toEqual(codeId2);

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

    const contractInfo1 = await directClient.getContract(contractAddress);
    console.info(contractInfo1);
    console.info(`contractInfo1 admin is same as faucet address0: ${contractInfo1.admin === signer}`);
    expect(contractInfo1.admin).toEqual(signer);

    // execute migrate msg
    const newVerifier = makeRandomAddress();
    // I don't think this is a valid migrate message for cw20, only hackatom
    const migrateMsg = { admin: newVerifier };

    const { logs, events, height, gasUsed, gasWanted } = await directClient.migrate(
      signer,
      contractAddress,
      codeId2,
      migrateMsg,
      "auto"
    );
    console.info(`height: ${height}`);
    console.info(`gasUsed: ${gasUsed}`);
    console.info(`gasWanted: ${gasWanted}`);

    // logs and events
    console.info(`logs: ${JSON.stringify(logs)}`);
    console.info(`events: ${JSON.stringify(events)}`);

    const contractInfo2 = await directClient.getContract(contractAddress);
    console.info(contractInfo2);
    // we change the code ID, not the admin
    expect(contractInfo2.codeId).toEqual(codeId2);
    expect(contractInfo2.admin).toEqual(signer);

    // Let's change the admin now
    await directClient.updateAdmin(signer, contractAddress, newVerifier, "auto");
    const contractInfo3 = await directClient.getContract(contractAddress);
    console.info(contractInfo3);
    // we change the admin, not the code ID
    expect(contractInfo3.codeId).toEqual(codeId2);
    expect(contractInfo3.admin).toEqual(newVerifier);
  });
});
