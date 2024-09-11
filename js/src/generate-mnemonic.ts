import { generateMnemonic, mnemonicToAddr } from "./utils";

(async () => {
    const mnemonic = generateMnemonic();
    const addr = await mnemonicToAddr(mnemonic);
    console.log(`Mnemonic: ${mnemonic}`);
    console.log(`Address: ${addr}`);
})();