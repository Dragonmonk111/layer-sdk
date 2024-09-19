mod rpc;
pub use rpc::*;

use crate::prelude::*;

cfg_if::cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        mod grpc_wasm;
        pub use grpc_wasm::*;
    } else {
        mod grpc_native;
        pub use grpc_native::*;
    }
}

pub fn apply_grpc_height<T>(req: &mut tonic::Request<T>, height: Option<u64>) -> Result<()> {
    if let Some(height) = height {
        req.metadata_mut()
            .insert("x-cosmos-block-height", height.to_string().try_into()?);
    }

    Ok(())
}
