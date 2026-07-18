use std::collections::{BTreeMap, BTreeSet};

use zk_clob_core::{
    Account, AccountId, StateMultiproof, StateRoot, build_state_multiproof_for, compute_state_root,
};

use crate::BatchBuildError;

pub struct AccountTree {
    accounts: Vec<Account>,
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

        Ok(Self { accounts, indices })
    }

    pub fn root(&self) -> StateRoot {
        compute_state_root(&self.accounts)
    }

    pub fn account(&self, id: &AccountId) -> Option<&Account> {
        self.indices
            .get(id)
            .map(|index| &self.accounts[*index as usize])
    }

    pub fn witness(
        &self,
        account_ids: &BTreeSet<AccountId>,
    ) -> Result<(Vec<Account>, StateMultiproof), BatchBuildError> {
        let mut indices = Vec::with_capacity(account_ids.len());
        for account_id in account_ids {
            indices.push(
                *self
                    .indices
                    .get(account_id)
                    .ok_or(BatchBuildError::UnknownAccount(*account_id))?,
            );
        }
        indices.sort_unstable();

        let accounts = indices
            .iter()
            .map(|index| self.accounts[*index as usize].clone())
            .collect();
        let proof = build_state_multiproof_for(&self.accounts, &indices)
            .map_err(|_| BatchBuildError::InvalidStateProof)?;
        Ok((accounts, proof))
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
            self.accounts[index as usize] = account;
        }
        Ok(())
    }
}
