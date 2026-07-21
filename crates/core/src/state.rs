use serde::{Deserialize, Serialize};

use crate::{
    Account, AccountId, SettlementError, StateRoot,
    trees::patricia::{PatriciaError, PatriciaMerkleTree, PatriciaMultiproof},
};

pub type StateMultiproof = PatriciaMultiproof<AccountId>;

impl From<PatriciaError> for SettlementError {
    fn from(_: PatriciaError) -> Self {
        Self::InvalidStateMultiproof
    }
}

/// Complete account state held by the host.
pub struct State {
    accounts: Vec<Account>,
}

impl State {
    pub fn new(mut accounts: Vec<Account>) -> Self {
        accounts.sort_unstable_by_key(|account| *account.id());
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

    pub fn insert_account(&mut self, account: Account) -> bool {
        match self
            .accounts
            .binary_search_by_key(account.id(), |existing| *existing.id())
        {
            Ok(_) => false,
            Err(index) => {
                self.accounts.insert(index, account);
                true
            }
        }
    }

    pub fn root(&self) -> StateRoot {
        PatriciaMerkleTree::<Account>::compute_root(&self.accounts)
            .expect("complete state must not contain duplicate account IDs")
    }

    pub fn witness(&self) -> Result<StateWitness, SettlementError> {
        let account_ids: Vec<_> = self.accounts.iter().map(|account| *account.id()).collect();
        self.witness_for(&account_ids)
    }

    pub fn witness_for(&self, account_ids: &[AccountId]) -> Result<StateWitness, SettlementError> {
        let multiproof = PatriciaMerkleTree::<Account>::build_multiproof(&self.accounts, account_ids)?;
        let accounts = account_ids
            .iter()
            .filter_map(|account_id| {
                self.accounts
                    .binary_search_by_key(account_id, |account| *account.id())
                    .ok()
                    .map(|index| self.accounts[index].clone())
            })
            .collect();
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
        Self { accounts, multiproof }
    }

    pub fn accounts(&self) -> &[Account] {
        &self.accounts
    }

    pub fn accounts_mut(&mut self) -> &mut Vec<Account> {
        &mut self.accounts
    }

    pub(crate) fn into_accounts(self) -> Vec<Account> {
        self.accounts
    }

    pub(crate) fn root(&self) -> Result<StateRoot, SettlementError> {
        Ok(PatriciaMerkleTree::<Account>::compute_root_from_proof(
            &self.accounts,
            &self.multiproof,
        )?)
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{Address, B256};

    use super::*;
    use crate::{AssetBalance, AssetId};

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
    fn state_sorts_accounts_by_id() {
        let state = State::new(vec![
            Account::new(BOB, vec![AssetBalance::new(ASSET, 20)], 0),
            Account::new(ALICE, vec![AssetBalance::new(ASSET, 10)], 0),
        ]);

        assert_eq!(state.account(0).map(Account::id), Some(&ALICE));
        assert_eq!(state.account(1).map(Account::id), Some(&BOB));
    }

    #[test]
    fn shared_proof_reconstructs_patricia_tree_root() {
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
    fn rejects_a_side_node_hiding_a_selected_key() {
        let state = State::new(accounts());
        let mut witness = state.witness().unwrap();
        let proof = &witness.multiproof;
        let mut side_nodes = proof.side_nodes().to_vec();
        side_nodes.push(crate::trees::patricia::PatriciaSubtree::new(
            StateRoot::ZERO,
            ALICE,
            ALICE,
        ));
        witness.multiproof = StateMultiproof::new(proof.leaf_keys().to_vec(), side_nodes);

        assert!(matches!(witness.root(), Err(SettlementError::InvalidStateMultiproof)));
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
        let witness = state.witness_for(&[ALICE, DAVE]).unwrap();

        assert_eq!(witness.root().unwrap(), state.root());
    }
}
