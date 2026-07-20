use std::collections::{BTreeMap, BTreeSet};

use zk_clob_core::{Account, AccountId, State, StateRoot, StateWitness};

use crate::BatchBuildError;

pub struct AccountTree {
    state: State,
    indices: BTreeMap<AccountId, u32>,
}

impl AccountTree {
    pub fn new(accounts: Vec<Account>) -> Result<Self, BatchBuildError> {
        let state = State::new(accounts);
        let mut indices = BTreeMap::new();
        for index in 0..state.len() {
            let index = u32::try_from(index).map_err(|_| BatchBuildError::AccountIndexOverflow)?;
            let account = state
                .account(index)
                .expect("index generated from state length must exist");
            if indices.insert(*account.id(), index).is_some() {
                return Err(BatchBuildError::DuplicateAccount(*account.id()));
            }
        }

        Ok(Self { state, indices })
    }

    pub fn root(&self) -> StateRoot {
        self.state.root()
    }

    pub fn account(&self, id: &AccountId) -> Option<&Account> {
        self.indices
            .get(id)
            .and_then(|index| self.state.account(*index))
    }

    pub fn witness(
        &self,
        account_ids: &BTreeSet<AccountId>,
    ) -> Result<StateWitness, BatchBuildError> {
        self.state
            .witness_for(&account_ids.iter().copied().collect::<Vec<_>>())
            .map_err(|_| BatchBuildError::InvalidStateProof)
    }

    pub fn apply(&mut self, updated_accounts: Vec<Account>) -> Result<(), BatchBuildError> {
        let mut seen = BTreeSet::new();
        let mut replacements = Vec::with_capacity(updated_accounts.len());
        for account in updated_accounts {
            if !seen.insert(*account.id()) {
                return Err(BatchBuildError::DuplicateAccount(*account.id()));
            }
            replacements.push((self.indices.get(account.id()).copied(), account));
        }
        let mut insertions = Vec::new();
        for (index, account) in replacements {
            let account_id = *account.id();
            if let Some(index) = index {
                if !self.state.replace_account(index, account) {
                    return Err(BatchBuildError::UnknownAccount(account_id));
                }
            } else {
                insertions.push(account);
            }
        }
        for account in insertions {
            let account_id = *account.id();
            if !self.state.insert_account(account) {
                return Err(BatchBuildError::DuplicateAccount(account_id));
            }
        }
        self.indices.clear();
        for index in 0..self.state.len() {
            let index = u32::try_from(index).map_err(|_| BatchBuildError::AccountIndexOverflow)?;
            let account = self
                .state
                .account(index)
                .expect("index generated from state length must exist");
            self.indices.insert(*account.id(), index);
        }
        Ok(())
    }
}
