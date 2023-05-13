use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::iter;
use std::iter::Peekable;
use std::ops::{Bound, RangeBounds};

use cosmwasm_std::{Order, Record};

use crate::ReadonlyStorage;
use pulsar_std::{GasMeter, GasResult};

/// The BTreeMap specific key-value pair reference type, as returned by BTreeMap<Vec<u8>, T>::range.
/// This is internal as it can change any time if the map implementation is swapped out.
type BTreeMapPairRef<'a, T = Vec<u8>> = (&'a Vec<u8>, &'a T);

// pub fn transactional<F, T, E>(base: &mut dyn Storage, action: F) -> Result<T, E>
// where
//     F: FnOnce(&mut dyn Storage, &dyn Storage) -> Result<T, E>,
// {
//     let mut cache = StorageTransaction::new(base);
//     let res = action(&mut cache, base)?;
//     cache.prepare().commit(base);
//     Ok(res)
// }

pub struct PulsarTransaction<'a> {
    /// read-only access to backing storage
    storage: Box<dyn ReadonlyStorage + 'a>,
    /// these are local changes not flushed to backing storage
    local_state: BTreeMap<Vec<u8>, Delta>,
}

impl<'a> PulsarTransaction<'a> {
    pub fn new(storage: Box<dyn ReadonlyStorage + 'a>) -> Self {
        PulsarTransaction {
            storage,
            local_state: BTreeMap::new(),
        }
    }

    /// prepares this transaction to be committed to storage
    /// Consumes local cache and converts it to something than can be applied to another storage
    pub fn prepare(self) -> RepLog {
        let ops_log = self.local_state.into_iter().map(Op::from_delta).collect();
        RepLog { ops_log }
    }
}

impl<'a> ReadonlyStorage for PulsarTransaction<'a> {
    fn get(&self, meter: &mut GasMeter, key: &[u8]) -> GasResult<Option<Vec<u8>>> {
        match self.local_state.get(key) {
            Some(val) => match val {
                Delta::Set { value } => Ok(Some(value.clone())),
                Delta::Delete {} => Ok(None),
            },
            None => self.storage.get(meter, key),
        }
    }

    /// range allows iteration over a set of keys, either forwards or backwards
    /// uses standard rust range notation, and eg db.range(b"foo"..b"bar") also works reverse
    fn range<'b>(
        &'b self,
        meter: &'b mut GasMeter,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: Order,
    ) -> GasResult<Box<dyn Iterator<Item = GasResult<Record>> + 'b>> {
        let bounds = range_bounds(start, end);

        // BTreeMap.range panics if range is start > end.
        // However, this cases represent just empty range and we treat it as such.
        let local: Box<dyn Iterator<Item = BTreeMapPairRef<Delta>>> =
            match (bounds.start_bound(), bounds.end_bound()) {
                (Bound::Included(start), Bound::Excluded(end)) if start > end => {
                    Box::new(iter::empty())
                }
                _ => {
                    let local_raw = self.local_state.range(bounds);
                    match order {
                        Order::Ascending => Box::new(local_raw),
                        Order::Descending => Box::new(local_raw.rev()),
                    }
                }
            };

        // TODO: make this proper
        let base = self.storage.range(meter, start, end, order)?;
        let merged = MergeOverlay::new(local, base, order);
        Ok(Box::new(merged))
    }

    fn abort(self) -> () {
        // nothing to do here
    }
}

// This isn't full Storage, commit is elsewhere
impl<'a> PulsarTransaction<'a> {
    pub fn set(&mut self, _meter: &mut GasMeter, key: &[u8], value: &[u8]) -> GasResult<()> {
        let delta = Delta::Set {
            value: value.to_vec(),
        };
        self.local_state.insert(key.to_vec(), delta);
        Ok(())
    }

    pub fn remove(&mut self, _meter: &mut GasMeter, key: &[u8]) -> GasResult<()> {
        let delta = Delta::Delete {};
        self.local_state.insert(key.to_vec(), delta);
        Ok(())
    }
}

pub struct RepLog {
    /// this is a list of changes to be written to backing storage upon commit
    pub(crate) ops_log: Vec<Op>,
}

/// Op is the user operation, which can be stored in the RepLog.
/// Currently Set or Delete.
pub enum Op {
    /// represents the `Set` operation for setting a key-value pair in storage
    Set {
        key: Vec<u8>,
        value: Vec<u8>,
    },
    Delete {
        key: Vec<u8>,
    },
}

impl Op {
    fn from_delta((key, delta): (Vec<u8>, Delta)) -> Self {
        match delta {
            Delta::Set { value } => Op::Set { key, value },
            Delta::Delete {} => Op::Delete { key },
        }
    }
}

/// Delta is the changes, stored in the local transaction cache.
/// This is either Set{value} or Delete{}. Note that this is the "value"
/// part of a BTree, so the Key (from the Op) is stored separately.
enum Delta {
    Set { value: Vec<u8> },
    Delete {},
}

struct MergeOverlay<'a, L, R>
where
    L: Iterator<Item = BTreeMapPairRef<'a, Delta>>,
    R: Iterator<Item = GasResult<Record>>,
{
    left: Peekable<L>,
    right: Peekable<R>,
    order: Order,
}

impl<'a, L, R> MergeOverlay<'a, L, R>
where
    L: Iterator<Item = BTreeMapPairRef<'a, Delta>>,
    R: Iterator<Item = GasResult<Record>>,
{
    fn new(left: L, right: R, order: Order) -> Self {
        MergeOverlay {
            left: left.peekable(),
            right: right.peekable(),
            order,
        }
    }

    fn pick_match(&mut self, lkey: Vec<u8>, rkey: Vec<u8>) -> Option<GasResult<Record>> {
        // compare keys - result is such that Ordering::Less => return left side
        let order = match self.order {
            Order::Ascending => lkey.cmp(&rkey),
            Order::Descending => rkey.cmp(&lkey),
        };

        // left must be translated and filtered before return, not so with right
        match order {
            Ordering::Less => self.take_left(),
            Ordering::Equal => {
                //
                let _ = self.right.next();
                self.take_left()
            }
            Ordering::Greater => self.right.next(),
        }
    }

    /// take_left must only be called when we know self.left.next() will return Some
    fn take_left(&mut self) -> Option<GasResult<Record>> {
        let (lkey, lval) = self.left.next().unwrap();
        match lval {
            Delta::Set { value } => Some(Ok((lkey.clone(), value.clone()))),
            Delta::Delete {} => self.next(),
        }
    }
}

impl<'a, L, R> Iterator for MergeOverlay<'a, L, R>
where
    L: Iterator<Item = BTreeMapPairRef<'a, Delta>>,
    R: Iterator<Item = GasResult<Record>>,
{
    type Item = GasResult<Record>;

    fn next(&mut self) -> Option<Self::Item> {
        // TODO: charge proper gas

        let (left, right) = (self.left.peek(), self.right.peek());
        match (left, right) {
            (Some(litem), Some(ritem)) => {
                let (lkey, _) = litem;
                let rkey = match ritem {
                    Ok((k, _)) => k,
                    Err(e) => return Some(Err(e.clone())),
                };

                // we just use cloned keys to avoid double mutable references
                // (we must release the return value from peek, before beginning to call next or other mut methods
                let (l, r) = (lkey.to_vec(), rkey.to_vec());
                self.pick_match(l, r)
            }
            (Some(_), None) => self.take_left(),
            (None, Some(_)) => self.right.next(),
            (None, None) => None,
        }
    }
}

fn range_bounds(start: Option<&[u8]>, end: Option<&[u8]>) -> impl RangeBounds<Vec<u8>> {
    (
        start.map_or(Bound::Unbounded, |x| Bound::Included(x.to_vec())),
        end.map_or(Bound::Unbounded, |x| Bound::Excluded(x.to_vec())),
    )
}
