import { DirectSecp256k1HdWallet } from "@cosmjs/proto-signing";
import {
    assertIsDeliverTxSuccess,
    SigningStargateClient,
  } from "@cosmjs/stargate";
import { Comet38Client } from "@cosmjs/tendermint-rpc";
import { MsgSend } from "cosmjs-types/cosmos/bank/v1beta1/tx";
import {
defaultSendFee,
defaultSigningClientOptions,
DENOM,
PREFIX,
faucet,
localNet,
} from "./testutils.spec";

const addr = process.argv[2];
if(!addr || addr === "") {
    console.error('Please provide address');
    process.exit(1);
}
const amount = Math.round(Number(process.argv[3]));

if(!amount || amount <= 0 || isNaN(amount)) {
    console.error('Please provide a valid amount as second argument');
    process.exit(1);
}

(async () => {
    const faucetWallet = await DirectSecp256k1HdWallet.fromMnemonic(faucet.mnemonic, {prefix: PREFIX});
    const faucetAddr = (await faucetWallet.getAccounts())[0].address;

    const tendermintClient = await Comet38Client.connect(localNet.tendermintUrl);
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
