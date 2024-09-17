import { generateMnemonic, mnemonicToAddr } from "./utils";

(async () => {
  const mnemonic = generateMnemonic();
  const addr = await mnemonicToAddr(mnemonic);
  console.info(`Mnemonic: ${mnemonic}`);
  console.info(`Address: ${addr}`);
})();
