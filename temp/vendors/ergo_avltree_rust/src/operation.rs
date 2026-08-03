use anyhow::{anyhow, Result};
use byteorder::{BigEndian, ByteOrder};
use bytes::Bytes;

pub const DIGEST_LENGTH: usize = 32;

pub type ADKey = Bytes;
pub type ADValue = Bytes;
pub type ADDigest = Bytes;
pub type Digest32 = [u8; DIGEST_LENGTH];

#[derive(Debug, Clone)]
pub enum Operation {
    Lookup(ADKey),
    UnknownModification(ADKey),
    Insert(KeyValue),
    Update(KeyValue),
    InsertOrUpdate(KeyValue),
    UpdateLongBy(KeyDelta),
    Remove(ADKey),
    RemoveIfExists(ADKey),
}

#[derive(Debug, Clone)]
pub struct KeyDelta {
    pub key: ADKey,
    pub delta: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyValue {
    pub key: ADKey,
    pub value: ADValue,
}

impl Operation {
    pub fn key(&self) -> ADKey {
        let key = match self {
            Operation::Lookup(key) => key,
            Operation::UnknownModification(key) => key,
            Operation::Insert(kv) => &kv.key,
            Operation::InsertOrUpdate(kv) => &kv.key,
            Operation::Update(kv) => &kv.key,
            Operation::UpdateLongBy(delta) => &delta.key,
            Operation::Remove(key) => key,
            Operation::RemoveIfExists(key) => key,
        };
        key.clone()
    }

    pub fn value(&self) -> Option<ADValue> {
        match self {
            Operation::Lookup(_key) => None,
            Operation::UnknownModification(_key) => None,
            Operation::Insert(kv) => Some(kv.value.clone()),
            Operation::InsertOrUpdate(kv) => Some(kv.value.clone()),
            Operation::Update(kv) => Some(kv.value.clone()),
            Operation::UpdateLongBy(_delta) => None,
            Operation::Remove(_key) => None,
            Operation::RemoveIfExists(_key) => None,
        }
    }

    pub fn update_fn(&self, old_value: Option<ADValue>) -> Result<Option<ADValue>> {
        match self {
            Operation::Lookup(_) => Ok(None),
            Operation::UnknownModification(_) => Ok(old_value),
            Operation::Insert(kv) => match old_value {
                None => Ok(Some(kv.value.clone())),
                // JVM / Basis-compatible behavior: AvlTree.insert updates an existing key.
                Some(_) => Ok(Some(kv.value.clone())),
            },
            Operation::Update(kv) => match old_value {
                None => Err(anyhow!("Key {:?} does not exists", kv.key)),
                Some(_) => Ok(Some(kv.value.clone())),
            },
            Operation::InsertOrUpdate(kv) => Ok(Some(kv.value.clone())),
            Operation::Remove(key) => match old_value {
                None => Err(anyhow!("Key {:?} does not exists", key)),
                Some(_) => Ok(None),
            },
            Operation::RemoveIfExists(_key) => Ok(None),
            /*
             * If the key exists in the tree, add delta to its value, fail if
             * the result is negative, and remove the key if the result is equal to 0.
             * If the key does not exist in the tree, treat it as if its value is 0:
             * insert the key with value delta if delta is positive,
             * fail if delta is negative, and do nothing if delta is 0.
             */
            Operation::UpdateLongBy(kv) => match old_value {
                m if kv.delta == 0 => Ok(m),
                None if kv.delta > 0 => Ok(Some(Bytes::copy_from_slice(&kv.delta.to_be_bytes()))),
                None if kv.delta < 0 => Err(anyhow!("Trying to decrease non-existing value")),
                Some(old) => {
                    let new_val = BigEndian::read_i64(&old) + kv.delta;
                    if new_val == 0 {
                        Ok(None)
                    } else if new_val > 0 {
                        Ok(Some(Bytes::copy_from_slice(&new_val.to_be_bytes())))
                    } else {
                        Err(anyhow!("New value is negative"))
                    }
                }
                None => Ok(None), // should not happen, but rust compiler ca not infer it
            },
        }
    }
}
