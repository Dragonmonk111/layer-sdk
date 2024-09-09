import { FaucetClient } from "@cosmjs/faucet-client";
import { StargateClient } from "@cosmjs/stargate";
import { Tendermint37Client } from "@cosmjs/tendermint-rpc";

import {
    DENOM,
    faucetUrl,
    makeRandomAddress,
    localNet,
} from "./testutils.spec";
  

describe("Faucet works", () =>
    describe("Get tokens", () => {
        it("New account", async () => {
            const tendermintClient = await Tendermint37Client.connect(localNet.tendermintUrl);
            const client = await StargateClient.create(tendermintClient);
            const beneficiaryAddress = makeRandomAddress();

            // ensure it is empty
            const before = await client.getBalance(beneficiaryAddress, DENOM);
            expect(before).toEqual({
              denom: DENOM,
              amount: "0",
            });

            // request from faucet
            const faucet = new FaucetClient(faucetUrl);
            await faucet.credit(beneficiaryAddress, DENOM);

            // check it arrives
            const after = await client.getBalance(beneficiaryAddress, DENOM);
            expect(after).toEqual({
              denom: DENOM,
              amount: "10000000",
            });


        })
    })
);