import { DirectSecp256k1HdWallet } from "@cosmjs/proto-signing";
import { assertIsDeliverTxSuccess, SigningStargateClient } from "@cosmjs/stargate";
import { Comet38Client } from "@cosmjs/tendermint-rpc";

import { defaultSendFee, defaultSigningClientOptions, DENOM, faucet, localNet, PREFIX } from "./testutils.spec";

const DefaultAmount = "1000000";
const addr = process.argv[2];

if (!addr || addr === "") {
  console.error("Please provide address");
  process.exit(1);
}

let amountStr = process.argv[3];
if (!amountStr || amountStr === "") {
  console.info(`no amount provided, defaulting to ${DefaultAmount}`);
  amountStr = DefaultAmount;
}
const amount = Math.round(Number(amountStr));

if (amount <= 0 || isNaN(amount)) {
  console.error(`Please provide a valid amount as second argument (or leave it off for default of ${DefaultAmount})`);
  process.exit(1);
}

(async () => {
  const faucetWallet = await DirectSecp256k1HdWallet.fromMnemonic(faucet.mnemonic, { prefix: PREFIX });
  const faucetAddr = (await faucetWallet.getAccounts())[0].address;

  const tendermintClient = await Comet38Client.connect(localNet.tendermintUrl);
  const client = await SigningStargateClient.createWithSigner(
    tendermintClient,
    faucetWallet,
    defaultSigningClientOptions
  );

  const balanceBefore = await client.getBalance(addr, DENOM);
  const result = await client.sendTokens(
    faucetAddr,
    addr,
    [{ amount: amount.toString(), denom: DENOM }],
    defaultSendFee,
    "faucet tap"
  );
  assertIsDeliverTxSuccess(result);
  const balanceAfter = await client.getBalance(addr, DENOM);

  const coin = `${Number(balanceAfter.amount) - Number(balanceBefore.amount)}${DENOM}`;

  console.log(`Sent ${coin} from ${faucetAddr} to ${addr}`);
  console.log(`current balance: ${balanceAfter.amount}${DENOM}`);
})();
