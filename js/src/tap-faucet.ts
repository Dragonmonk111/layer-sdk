import { DirectSecp256k1HdWallet } from "@cosmjs/proto-signing";
import {
    assertIsDeliverTxSuccess,
    SigningStargateClient,
  } from "@cosmjs/stargate";
import { Tendermint37Client } from "@cosmjs/tendermint-rpc";
import { MsgSend } from "cosmjs-types/cosmos/bank/v1beta1/tx";
import {
defaultSendFee,
defaultSigningClientOptions,
DENOM,
PREFIX,
faucet,
localNet,
} from "./testutils.spec";

const addrOrMnemonic = process.argv[2];
if(!addrOrMnemonic || addrOrMnemonic === "") {
    console.error('Please provide address or seed phrase as argument');
    process.exit(1);
}
const amount = Math.round(Number(process.argv[3]));

if(!amount || amount <= 0 || isNaN(amount)) {
    console.error('Please provide a valid amount as second argument');
    process.exit(1);
}

(async () => {


    const addr = (addrOrMnemonic.includes(" ") ? await mnemonicToAddr(addrOrMnemonic) : addrOrMnemonic);
    const faucetWallet = await DirectSecp256k1HdWallet.fromMnemonic(faucet.mnemonic, {prefix: PREFIX});
    const faucetAddr = (await faucetWallet.getAccounts())[0].address;

    const tendermintClient = await Tendermint37Client.connect(localNet.tendermintUrl);
    const client = await SigningStargateClient.createWithSigner(
      tendermintClient,
      faucetWallet,
      defaultSigningClientOptions
    );

    const balanceBefore = await client.getBalance(addr, DENOM);
    const result = await client.sendTokens(faucetAddr, addr, [{amount: amount.toString(), denom: DENOM}], defaultSendFee, "faucet tap");
    assertIsDeliverTxSuccess(result);
    const balanceAfter = await client.getBalance(addr, DENOM);

    const coin = `${Number(balanceAfter.amount) - Number(balanceBefore.amount)}${DENOM}`;

    console.log(`Sent ${coin} from ${faucetAddr} to ${addr}`);
    console.log(`current balance: ${balanceAfter.amount}${DENOM}`);
})();

async function mnemonicToAddr(mnemonic: string): Promise<string> {
    return DirectSecp256k1HdWallet.fromMnemonic(mnemonic, {prefix: PREFIX})
        .then(wallet => wallet.getAccounts())
        .then((accounts) => accounts[0].address);
}