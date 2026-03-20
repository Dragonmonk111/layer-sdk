mod extensions;
mod protos;

pub use protos::*;

/// Compiled FileDescriptorSet for all Layer proto services — used for gRPC server reflection.
#[cfg(feature = "tonic")]
pub const FILE_DESCRIPTOR_SET: &[u8] =
    include_bytes!("protos/service_descriptor.bin");
