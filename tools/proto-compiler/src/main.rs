
fn main() {
    let proto = "../../proto/cosmos/crypto/secp256k1/keys.proto";
    // let out = "../../packages/proto/src";
    prost_build::compile_protos(&[proto], &[]).unwrap();
}
