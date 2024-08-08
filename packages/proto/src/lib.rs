mod protos;

pub use protos::*;

use protos::layer::sync::v1::{state_change::Event, BlockWrites, StateChange, WriteData};

impl std::fmt::Display for WriteData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = stringify_or_hex(&self.value);
        write!(
            f,
            r#"WriteData {{ module: "{}", bucket: "{}", keys: {:?}, value: {} }}"#,
            self.module, self.bucket, self.keys, value
        )
    }
}

impl std::fmt::Display for BlockWrites {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BlockWrites( {} )", self.height)?;
        for evt in self.events.iter() {
            write!(f, "\n")?;
            write!(f, "{}", evt)?;
        }
        Ok(())
    }
}

impl std::fmt::Display for StateChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.event {
            Some(Event::WriteState(data)) => {
                let value = stringify_or_hex(&data.value);
                write!(
                    f,
                    r#"  Write: module: "{}", bucket: "{}", keys: {:?}, value: {}"#,
                    data.module, data.bucket, data.keys, value
                )
            }
            Some(Event::DeleteState(data)) => {
                write!(
                    f,
                    r#"  Delete: module: "{}", bucket: "{}", keys: {:?}"#,
                    data.module, data.bucket, data.keys
                )
            }
            // silently ignore case which should never happen
            None => Ok(()),
        }
    }
}

// TODO: move this to standard utils (also in storage/src/traits.rs)
pub fn stringify_or_hex(input: &[u8]) -> String {
    std::str::from_utf8(input).map_or_else(|_| hex::encode_upper(input), |x| x.to_string())
}
