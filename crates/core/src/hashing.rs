use sha2::{Digest as _, Sha256};

use crate::{Account, StateRoot};

const DOMAIN_STATE: &[u8] = b"ZKCLOB_STATE_V1";

pub trait Sha256Hash {
    fn update_hash(&self, hasher: &mut Sha256);
}

pub fn compute_state_root(accounts: &[Account]) -> StateRoot {
    let mut hasher = Sha256::new();
    hasher.update(DOMAIN_STATE);
    hasher.update((accounts.len() as u64).to_be_bytes());

    for account in accounts {
        account.update_hash(&mut hasher);
    }

    StateRoot::new(hasher.finalize().into())
}
