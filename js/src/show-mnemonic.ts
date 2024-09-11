import { mnemonicToAddr } from "./utils";

const mnemonic = process.argv[2];

if(!mnemonic || mnemonic === "" || !mnemonic.includes(" ")) {
    console.error('Please provide mnemonic as argument in quotes');
    process.exit(1);
}

(async () => {
    const addr = await mnemonicToAddr(mnemonic);
    console.log(`Address: ${addr}`);
})();