import { Bip39, Random } from "@cosmjs/crypto";
import { DirectSecp256k1HdWallet } from "@cosmjs/proto-signing";

import { PREFIX } from "./testutils.spec";

export async function mnemonicToAddr(mnemonic: string): Promise<string> {
  const wallet = await DirectSecp256k1HdWallet.fromMnemonic(mnemonic, { prefix: PREFIX });
  const accounts = await wallet.getAccounts();
  return accounts[0].address;
}

export function generateMnemonic(): string {
  return Bip39.encode(Random.getBytes(32)).toString();
}
