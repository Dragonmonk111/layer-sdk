use sha2::{Digest, Sha256};

pub struct FastHasher(Sha256);

impl FastHasher {
    pub fn new(old_hash: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(old_hash);
        Self(hasher)
    }

    pub fn set(&mut self, key: &[u8], value: &[u8]) {
        self.0.update(b"set");
        self.0.update(key);
        self.0.update(value);
    }

    pub fn remove(&mut self, key: &[u8]) {
        self.0.update(b"remove");
        self.0.update(key);
    }

    pub fn hash(self) -> Vec<u8> {
        self.0.finalize().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use crate::FastHasher;

    #[test]
    fn reproducable() {
        let mut a = FastHasher::new(&[0u8; 32]);
        a.set(b"foo", b"bar");
        a.remove(b"super");
        let hashed_a = a.hash();
        assert_eq!(hashed_a.len(), 32);

        let mut b = FastHasher::new(&[0u8; 32]);
        b.set(b"foo", b"bar");
        b.remove(b"super");
        let hashed_b = b.hash();
        assert_eq!(hashed_a, hashed_b);
    }

    #[test]
    fn different_for_different_data() {
        let mut a = FastHasher::new(&[0u8; 32]);
        a.set(b"foo", b"bar");
        a.remove(b"super");
        let hashed_a = a.hash();
        assert_eq!(hashed_a.len(), 32);

        let mut b = FastHasher::new(&[0u8; 32]);
        b.set(b"foo", b"bard");
        b.remove(b"super");
        let hashed_b = b.hash();
        assert_ne!(hashed_a, hashed_b);

        let mut c = FastHasher::new(&[0u8; 32]);
        c.set(b"fot", b"bar");
        c.remove(b"super");
        let hashed_c = c.hash();
        assert_ne!(hashed_a, hashed_c);

        let mut d = FastHasher::new(&[0u8; 32]);
        d.set(b"foo", b"bar");
        d.remove(b"supes");
        let hashed_d = d.hash();
        assert_ne!(hashed_a, hashed_d);
    }

    #[test]
    fn different_for_different_order() {
        let mut a = FastHasher::new(&[0u8; 32]);
        a.set(b"foo", b"bar");
        a.remove(b"super");
        let hashed_a = a.hash();
        assert_eq!(hashed_a.len(), 32);

        let mut b = FastHasher::new(&[0u8; 32]);
        b.remove(b"super");
        b.set(b"foo", b"bar");
        let hashed_b = b.hash();
        assert_ne!(hashed_a, hashed_b);
    }

    #[test]
    fn different_for_different_start() {
        let mut a = FastHasher::new(&[0u8; 32]);
        a.set(b"foo", b"bar");
        a.remove(b"super");
        let hashed_a = a.hash();
        assert_eq!(hashed_a.len(), 32);

        let mut b = FastHasher::new(&[1u8; 32]);
        b.set(b"foo", b"bar");
        b.remove(b"super");
        let hashed_b = b.hash();
        assert_ne!(hashed_a, hashed_b);
    }

    // FIXME: this would be a nice to have...
    // #[test]
    // fn bacth_size_irrelevant() {
    //     let mut a = FastHasher::new(&[0u8; 32]);
    //     a.set(b"foo", b"bar");
    //     let temp = a.hash();
    //     let mut b = FastHasher::new(&temp);
    //     b.remove(b"super");
    //     let hashed = b.hash();
    //     assert_eq!(hashed.len(), 32);

    //     let mut group = FastHasher::new(&[0u8; 32]);
    //     group.set(b"foo", b"bar");
    //     group.remove(b"super");
    //     let hashed_group = group.hash();
    //     assert_eq!(hashed, hashed_group);
    // }
}
