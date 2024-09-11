import { SigningCosmWasmClient } from "@cosmjs/cosmwasm-stargate";
import { DirectSecp256k1HdWallet } from "@cosmjs/proto-signing";
import { Comet38Client} from "@cosmjs/tendermint-rpc";
import fs from "fs";

import { defaultSigningClientOptions, defaultWalletOptions, faucet, localNet } from "./testutils.spec";

describe("Upload Abstract Manager", () => {
  describe("big wasm file", () => {
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
      const wasmBuf = fs.readFileSync(__dirname + "/../../packages/app/fixtures/abstract_manager.wasm");
      const wasm = new Uint8Array(wasmBuf); // not sure if this is needed but something is odd here (always out of gas)
      console.info(`Wasm size: ${wasm.length} bytes`);

      const uploadReceipt = await client.upload(signer, wasm, "auto");
      expect(uploadReceipt).toBeTruthy();
      const { codeId } = uploadReceipt;
      expect(codeId).toBeTruthy();
    });
  });
});
