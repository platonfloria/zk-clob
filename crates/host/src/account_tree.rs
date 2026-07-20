use std::collections::{BTreeMap, BTreeSet};

use zk_clob_core::{Account, AccountId, State, StateRoot, StateWitness};

use crate::BatchBuildError;

pub struct AccountTree {
    state: State,
    indices: BTreeMap<AccountId, u32>,
}

impl AccountTree {
    pub fn new(mut accounts: Vec<Account>) -> Result<Self, BatchBuildError> {
        accounts.sort_unstable_by_key(|account| *account.id());

        let mut indices = BTreeMap::new();
        for (index, account) in accounts.iter().enumerate() {
            let index = u32::try_from(index).map_err(|_| BatchBuildError::AccountIndexOverflow)?;
            if indices.insert(*account.id(), index).is_some() {
                return Err(BatchBuildError::DuplicateAccount(*account.id()));
            }
        }

        Ok(Self {
            state: State::new(accounts),
            indices,
        })
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
        for account_id in account_ids {
            if !self.indices.contains_key(account_id) {
                return Err(BatchBuildError::UnknownAccount(*account_id));
            }
        }

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
            let index = *self
                .indices
                .get(account.id())
                .ok_or(BatchBuildError::UnknownAccount(*account.id()))?;
            replacements.push((index, account));
        }
        for (index, account) in replacements {
            let account_id = *account.id();
            if !self.state.replace_account(index, account) {
                return Err(BatchBuildError::UnknownAccount(account_id));
            }
        }
        Ok(())
    }
}
