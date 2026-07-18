use sha2::{Digest as _, Sha256};

use crate::{
    Account, SettlementError, StateMultiproof, StateRoot,
    hashing::{DomainSha256Hash, Sha256Hash},
};

struct EmptyLeaf;

impl Sha256Hash for EmptyLeaf {
    fn update_hash(&self, _hasher: &mut Sha256) {}
}

impl DomainSha256Hash for EmptyLeaf {
    const DOMAIN: &'static [u8] = b"ZKCLOB_DMT_EMPTY_LEAF_V1";
}

impl Sha256Hash for (&StateRoot, &StateRoot) {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.0);
        hasher.update(self.1);
    }
}

impl DomainSha256Hash for (&StateRoot, &StateRoot) {
    const DOMAIN: &'static [u8] = b"ZKCLOB_DMT_NODE_V1";
}

impl Sha256Hash for (u32, &StateRoot) {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.0.to_be_bytes());
        hasher.update(self.1);
    }
}

impl DomainSha256Hash for (u32, &StateRoot) {
    const DOMAIN: &'static [u8] = b"ZKCLOB_DMT_ROOT_V1";
}

fn tree_width(leaf_count: usize) -> usize {
    leaf_count.max(1).next_power_of_two()
}

fn build_levels(accounts: &[Account]) -> Vec<Vec<StateRoot>> {
    let width = tree_width(accounts.len());
    let mut levels = Vec::new();
    let mut level: Vec<_> = accounts.iter().map(DomainSha256Hash::hash).collect();
    level.resize(width, EmptyLeaf.hash());
    levels.push(level);

    while levels.last().expect("leaf level exists").len() > 1 {
        let previous = levels.last().expect("previous level exists");
        let next = previous
            .chunks_exact(2)
            .map(|pair| (&pair[0], &pair[1]).hash())
            .collect();
        levels.push(next);
    }
    levels
}

fn subtree_root(levels: &[Vec<StateRoot>], start: usize, width: usize) -> StateRoot {
    let level = width.trailing_zeros() as usize;
    levels[level][start / width]
}

fn build_side_nodes(
    levels: &[Vec<StateRoot>],
    selected: &[u32],
    start: usize,
    width: usize,
    side_nodes: &mut Vec<StateRoot>,
) {
    if selected.is_empty() {
        side_nodes.push(subtree_root(levels, start, width));
        return;
    }
    if width == 1 {
        return;
    }

    let midpoint = start + width / 2;
    let split = selected.partition_point(|index| (*index as usize) < midpoint);
    build_side_nodes(levels, &selected[..split], start, width / 2, side_nodes);
    build_side_nodes(levels, &selected[split..], midpoint, width / 2, side_nodes);
}

fn root_from_proof(
    accounts: &[Account],
    indices: &[u32],
    start: usize,
    width: usize,
    side_nodes: &mut impl Iterator<Item = StateRoot>,
) -> Result<StateRoot, SettlementError> {
    if indices.is_empty() {
        return side_nodes
            .next()
            .ok_or(SettlementError::InvalidStateMultiproof);
    }
    if width == 1 {
        if accounts.len() != 1 || indices.len() != 1 || indices[0] as usize != start {
            return Err(SettlementError::InvalidStateMultiproof);
        }
        return Ok(accounts[0].hash());
    }

    let midpoint = start + width / 2;
    let split = indices.partition_point(|index| (*index as usize) < midpoint);
    let left = root_from_proof(
        &accounts[..split],
        &indices[..split],
        start,
        width / 2,
        side_nodes,
    )?;
    let right = root_from_proof(
        &accounts[split..],
        &indices[split..],
        midpoint,
        width / 2,
        side_nodes,
    )?;
    Ok((&left, &right).hash())
}

/// Computes the canonical dense Merkle root of all accounts in sorted ID order.
pub fn compute_state_root(accounts: &[Account]) -> StateRoot {
    let levels = build_levels(accounts);
    (
        u32::try_from(accounts.len()).expect("account count must fit in u32"),
        &levels.last().expect("root level exists")[0],
    )
        .hash()
}

/// Builds a multiproof when every account in the dense tree is supplied to the batch.
/// A host with persistent state may construct the same format for any sorted subset.
pub fn build_state_multiproof(accounts: &[Account]) -> StateMultiproof {
    let leaf_count = u32::try_from(accounts.len()).expect("account count must fit in u32");
    let leaf_indices: Vec<_> = (0..leaf_count).collect();
    build_state_multiproof_for(accounts, &leaf_indices)
        .expect("all account indices form a valid multiproof selection")
}

/// Builds a multiproof for a strictly increasing subset of account leaf indices.
pub fn build_state_multiproof_for(
    accounts: &[Account],
    leaf_indices: &[u32],
) -> Result<StateMultiproof, SettlementError> {
    let leaf_count = u32::try_from(accounts.len()).map_err(|_| SettlementError::TooManyAccounts)?;
    if leaf_count == 0
        || leaf_indices.is_empty()
        || leaf_indices.windows(2).any(|pair| pair[0] >= pair[1])
        || leaf_indices
            .last()
            .is_some_and(|index| *index >= leaf_count)
    {
        return Err(SettlementError::InvalidStateMultiproof);
    }

    let levels = build_levels(accounts);
    let mut side_nodes = Vec::new();
    build_side_nodes(
        &levels,
        leaf_indices,
        0,
        tree_width(accounts.len()),
        &mut side_nodes,
    );
    Ok(StateMultiproof::new(
        leaf_count,
        leaf_indices.to_vec(),
        side_nodes,
    ))
}

pub(crate) fn compute_state_root_from_proof(
    accounts: &[Account],
    proof: &StateMultiproof,
) -> Result<StateRoot, SettlementError> {
    let indices = proof.leaf_indices();
    if accounts.len() != indices.len()
        || proof.leaf_count() == 0
        || indices.windows(2).any(|pair| pair[0] >= pair[1])
        || indices
            .last()
            .is_some_and(|index| *index >= proof.leaf_count())
    {
        return Err(SettlementError::InvalidStateMultiproof);
    }

    let mut side_nodes = proof.side_nodes().iter().copied();
    let tree_root = root_from_proof(
        accounts,
        indices,
        0,
        tree_width(proof.leaf_count() as usize),
        &mut side_nodes,
    )?;
    if side_nodes.next().is_some() {
        return Err(SettlementError::InvalidStateMultiproof);
    }
    Ok((proof.leaf_count(), &tree_root).hash())
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
        let accounts = accounts();
        let proof = build_state_multiproof(&accounts);

        assert_eq!(
            compute_state_root_from_proof(&accounts, &proof).unwrap(),
            compute_state_root(&accounts)
        );
    }

    #[test]
    fn same_proof_commits_updated_account_leaves() {
        let mut accounts = accounts();
        let proof = build_state_multiproof(&accounts);
        let old_root = compute_state_root_from_proof(&accounts, &proof).unwrap();
        accounts[0].credit(ASSET, 1).unwrap();

        assert_ne!(
            compute_state_root_from_proof(&accounts, &proof).unwrap(),
            old_root
        );
    }

    #[test]
    fn rejects_unused_side_nodes() {
        let accounts = accounts();
        let proof = build_state_multiproof(&accounts);
        let mut side_nodes = proof.side_nodes().to_vec();
        side_nodes.push(StateRoot::ZERO);

        assert!(matches!(
            compute_state_root_from_proof(
                &accounts,
                &StateMultiproof::new(
                    proof.leaf_count(),
                    proof.leaf_indices().to_vec(),
                    side_nodes,
                ),
            ),
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
        let selected_accounts = vec![all_accounts[0].clone(), all_accounts[3].clone()];
        let proof = build_state_multiproof_for(&all_accounts, &[0, 3]).unwrap();

        assert_eq!(
            compute_state_root_from_proof(&selected_accounts, &proof).unwrap(),
            compute_state_root(&all_accounts)
        );
    }
}
