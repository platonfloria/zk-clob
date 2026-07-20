use serde::{Deserialize, Serialize};

use crate::{
    Account, SettlementError, StateRoot,
    dmt::{DenseMerkleError, DenseMerkleMultiproof as StateMultiproof, DenseMerkleTree},
};

impl From<DenseMerkleError> for SettlementError {
    fn from(value: DenseMerkleError) -> Self {
        match value {
            DenseMerkleError::TooManyLeaves => Self::TooManyAccounts,
            DenseMerkleError::InvalidMultiproof => Self::InvalidStateMultiproof,
        }
    }
}

/// Complete canonical account state held by the host.
pub struct State {
    accounts: Vec<Account>,
}

impl State {
    pub const fn new(accounts: Vec<Account>) -> Self {
        Self { accounts }
    }

    pub fn account(&self, index: u32) -> Option<&Account> {
        self.accounts.get(index as usize)
    }

    pub fn len(&self) -> usize {
        self.accounts.len()
    }

    pub fn replace_account(&mut self, index: u32, account: Account) -> bool {
        let Some(existing) = self.accounts.get_mut(index as usize) else {
            return false;
        };
        if existing.id() != account.id() {
            return false;
        }
        *existing = account;
        true
    }

    pub fn root(&self) -> StateRoot {
        DenseMerkleTree::<Account>::compute_root(&self.accounts)
    }

    pub fn witness(&self) -> Result<StateWitness, SettlementError> {
        let leaf_count = self.accounts.len() as u32;
        self.witness_for(&(0..leaf_count).collect::<Vec<_>>())
    }

    pub fn witness_for(&self, leaf_indices: &[u32]) -> Result<StateWitness, SettlementError> {
        let multiproof =
            DenseMerkleTree::<Account>::build_multiproof(&self.accounts, leaf_indices)?;
        let accounts = leaf_indices
            .iter()
            .map(|index| {
                self.accounts
                    .get(*index as usize)
                    .cloned()
                    .ok_or(SettlementError::InvalidStateMultiproof)
            })
            .collect::<Result<_, _>>()?;
        Ok(StateWitness::new(accounts, multiproof))
    }
}

/// Touched accounts and their proof against the complete account state.
#[derive(Deserialize, Serialize)]
pub struct StateWitness {
    accounts: Vec<Account>,
    multiproof: StateMultiproof,
}

impl StateWitness {
    pub const fn new(accounts: Vec<Account>, multiproof: StateMultiproof) -> Self {
        Self {
            accounts,
            multiproof,
        }
    }

    pub fn accounts(&self) -> &[Account] {
        &self.accounts
    }

    pub(crate) fn accounts_mut(&mut self) -> &mut Vec<Account> {
        &mut self.accounts
    }

    pub(crate) fn into_accounts(self) -> Vec<Account> {
        self.accounts
    }

    pub(crate) fn root(&self) -> Result<StateRoot, SettlementError> {
        Ok(DenseMerkleTree::<Account>::compute_root_from_proof(
            &self.accounts,
            &self.multiproof,
        )?)
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{Address, B256};

    use super::*;
    use crate::{AccountId, AssetBalance, AssetId};

    const ASSET: AssetId = AssetId::new(B256::new([9; 32]));
    const ALICE: AccountId = AccountId::new(Address::new([1; 20]));
    const BOB: AccountId = AccountId::new(Address::new([2; 20]));
    const CAROL: AccountId = AccountId::new(Address::new([3; 20]));
    const DAVE: AccountId = AccountId::new(Address::new([4; 20]));

    fn accounts() -> Vec<Account> {
        vec![
            Account::new(ALICE, vec![AssetBalance::new(ASSET, 10)], 0),
            Account::new(BOB, vec![AssetBalance::new(ASSET, 20)], 0),
        ]
    }

    #[test]
    fn shared_proof_reconstructs_dense_tree_root() {
        let state = State::new(accounts());
        let witness = state.witness().unwrap();

        assert_eq!(witness.root().unwrap(), state.root());
    }

    #[test]
    fn same_proof_commits_updated_account_leaves() {
        let state = State::new(accounts());
        let mut witness = state.witness().unwrap();
        let old_root = witness.root().unwrap();
        witness.accounts_mut()[0].credit(ASSET, 1).unwrap();

        assert_ne!(witness.root().unwrap(), old_root);
    }

    #[test]
    fn rejects_unused_side_nodes() {
        let state = State::new(accounts());
        let mut witness = state.witness().unwrap();
        let proof = &witness.multiproof;
        let mut side_nodes = proof.side_nodes().to_vec();
        side_nodes.push(StateRoot::ZERO);
        witness.multiproof = StateMultiproof::new(
            proof.leaf_count(),
            proof.leaf_indices().to_vec(),
            side_nodes,
        );

        assert!(matches!(
            witness.root(),
            Err(SettlementError::InvalidStateMultiproof)
        ));
    }

    #[test]
    fn subset_proof_reconstructs_full_tree_root() {
        let all_accounts = vec![
            Account::new(ALICE, vec![AssetBalance::new(ASSET, 10)], 0),
            Account::new(BOB, vec![AssetBalance::new(ASSET, 20)], 0),
            Account::new(CAROL, vec![AssetBalance::new(ASSET, 30)], 0),
            Account::new(DAVE, vec![AssetBalance::new(ASSET, 40)], 0),
        ];
        let state = State::new(all_accounts);
        let witness = state.witness_for(&[0, 3]).unwrap();

        assert_eq!(witness.root().unwrap(), state.root());
    }
}
