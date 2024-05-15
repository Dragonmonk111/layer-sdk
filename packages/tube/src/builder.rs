use crate::{signer::KeyConfig, DerivedKey, Slay3rTube};

const DEFAULT_MNEMONIC: &str = "";

#[derive(Clone, Debug, Default)]
pub struct Slay3rTubeBuilder {
    // set to a default
    mnemonic: Option<String>,
    // make a random temp-dir
    cache_dir: Option<String>,
    // set to a default
    key_config: KeyConfig,
}

impl Slay3rTubeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    // Finds a path from the cache_dir
    pub fn ensure_cache_dir(&self) -> Result<String, std::io::Error> {
        let _ = self.cache_dir;
        todo!();
    }

    pub fn build(self) -> Slay3rTube {
        let cache_dir = self.ensure_cache_dir().unwrap();
        let mnemonic = self
            .mnemonic
            .unwrap_or_else(|| DEFAULT_MNEMONIC.to_string());
        let signer = DerivedKey::new(mnemonic, self.key_config);
        Slay3rTube::new(&cache_dir, signer)
    }
}
