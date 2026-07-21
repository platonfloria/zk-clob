use alloy_primitives::B256;
use core::marker::PhantomData;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::{
    hashing::{DomainSha256Hash, Sha256Hash},
    trees::smt_empty_hashes::EMPTY_HASHES_BY_HEIGHT,
};

pub trait SparseMerkleKey: Copy + Ord {
    const BITS: usize;

    fn bit(&self, index: usize) -> bool;
}

impl<const N: usize> SparseMerkleKey for [u8; N] {
    const BITS: usize = N * 8;

    fn bit(&self, index: usize) -> bool {
        self[index / 8] & (1 << (7 - index % 8)) != 0
    }
}

pub trait SparseMerkleLeaf: DomainSha256Hash {
    type Key: SparseMerkleKey;

    fn key(&self) -> Self::Key;
}

#[derive(Debug, Eq, PartialEq)]
pub enum SparseMerkleError {
    InvalidMultiproof,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SparseMerkleMultiproof<K> {
    leaf_keys: Vec<K>,
    sibling_bitmap: Vec<u8>,
    side_nodes: Vec<B256>,
}

impl<K> SparseMerkleMultiproof<K> {
    pub const fn new(leaf_keys: Vec<K>, sibling_bitmap: Vec<u8>, side_nodes: Vec<B256>) -> Self {
        Self {
            leaf_keys,
            sibling_bitmap,
            side_nodes,
        }
    }

    pub fn leaf_keys(&self) -> &[K] {
        &self.leaf_keys
    }

    pub fn sibling_bitmap(&self) -> &[u8] {
        &self.sibling_bitmap
    }

    pub fn side_nodes(&self) -> &[B256] {
        &self.side_nodes
    }

    fn reader(&self) -> ProofReader<'_> {
        ProofReader {
            bitmap: &self.sibling_bitmap,
            bit_index: 0,
            side_nodes: self.side_nodes.iter(),
        }
    }
}

impl<K: Ord> SparseMerkleMultiproof<K> {
    fn validate_keys(&self) -> Result<(), SparseMerkleError> {
        if self.leaf_keys.is_empty() || self.leaf_keys.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(SparseMerkleError::InvalidMultiproof);
        }
        Ok(())
    }
}

pub struct SparseMerkleTree<L>(PhantomData<L>);

struct Node<'a> {
    left: &'a B256,
    right: &'a B256,
}

impl Sha256Hash for Node<'_> {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.left);
        hasher.update(self.right);
    }
}

impl DomainSha256Hash for Node<'_> {
    const DOMAIN: &'static [u8] = b"ZKCLOB_SMT_NODE_V1";
}

#[derive(Clone, Copy)]
struct ProofLeaf<'a, L: SparseMerkleLeaf> {
    key: L::Key,
    value: Option<&'a L>,
}

impl<'a, L: SparseMerkleLeaf> ProofLeaf<'a, L> {
    fn present(value: &'a L) -> Self {
        Self {
            key: value.key(),
            value: Some(value),
        }
    }

    const fn absent(key: L::Key) -> Self {
        Self { key, value: None }
    }

    fn hash(&self, empty_leaf: B256) -> B256 {
        self.value.map_or(empty_leaf, DomainSha256Hash::hash)
    }
}

fn empty_hash<L: SparseMerkleLeaf>(depth: usize) -> B256 {
    let height = L::Key::BITS
        .checked_sub(depth)
        .expect("depth must not exceed sparse Merkle key width");
    *EMPTY_HASHES_BY_HEIGHT
        .get(height)
        .expect("sparse Merkle key width must not exceed 160 bits")
}

fn split<L: SparseMerkleLeaf>(leaves: &[ProofLeaf<'_, L>], depth: usize) -> usize {
    leaves.partition_point(|leaf| !leaf.key.bit(depth))
}

fn subtree_root<L: SparseMerkleLeaf>(leaves: &[ProofLeaf<'_, L>], depth: usize) -> B256 {
    if leaves.is_empty() {
        return empty_hash::<L>(depth);
    }
    if depth == L::Key::BITS {
        return leaves[0].hash(empty_hash::<L>(depth));
    }

    let split = split(leaves, depth);
    let left = subtree_root::<L>(&leaves[..split], depth + 1);
    let right = subtree_root::<L>(&leaves[split..], depth + 1);
    Node {
        left: &left,
        right: &right,
    }
    .hash()
}

struct ProofBuilder {
    bitmap: Vec<u8>,
    bit_count: usize,
    side_nodes: Vec<B256>,
}

impl ProofBuilder {
    const fn new() -> Self {
        Self {
            bitmap: Vec::new(),
            bit_count: 0,
            side_nodes: Vec::new(),
        }
    }

    fn push_bit(&mut self, value: bool) {
        if self.bit_count % 8 == 0 {
            self.bitmap.push(0);
        }
        if value {
            self.bitmap[self.bit_count / 8] |= 1 << (7 - self.bit_count % 8);
        }
        self.bit_count += 1;
    }

    fn push_subtree(&mut self, root: B256, empty_root: B256) {
        let non_empty = root != empty_root;
        self.push_bit(non_empty);
        if non_empty {
            self.side_nodes.push(root);
        }
    }
}

fn build_proof<L: SparseMerkleLeaf>(
    all: &[ProofLeaf<'_, L>],
    selected: &[ProofLeaf<'_, L>],
    depth: usize,
    proof: &mut ProofBuilder,
) {
    if selected.is_empty() {
        let root = subtree_root::<L>(all, depth);
        proof.push_subtree(root, empty_hash::<L>(depth));
        return;
    }
    if depth == L::Key::BITS {
        return;
    }

    let all_split = split(all, depth);
    let selected_split = split(selected, depth);
    build_proof::<L>(&all[..all_split], &selected[..selected_split], depth + 1, proof);
    build_proof::<L>(&all[all_split..], &selected[selected_split..], depth + 1, proof);
}

struct ProofReader<'a> {
    bitmap: &'a [u8],
    bit_index: usize,
    side_nodes: core::slice::Iter<'a, B256>,
}

impl ProofReader<'_> {
    fn missing_subtree(&mut self, empty_root: B256) -> Result<B256, SparseMerkleError> {
        if self.bit_index >= self.bitmap.len() * 8 {
            return Err(SparseMerkleError::InvalidMultiproof);
        }
        let non_empty = self.bitmap[self.bit_index / 8] & (1 << (7 - self.bit_index % 8)) != 0;
        self.bit_index += 1;
        if non_empty {
            self.side_nodes
                .next()
                .copied()
                .ok_or(SparseMerkleError::InvalidMultiproof)
        } else {
            Ok(empty_root)
        }
    }

    fn finish(mut self) -> Result<(), SparseMerkleError> {
        let expected_bytes = self.bit_index.div_ceil(8);
        let trailing_bits_are_zero =
            self.bit_index % 8 == 0 || self.bitmap[expected_bytes - 1] & ((1 << (8 - self.bit_index % 8)) - 1) == 0;
        if expected_bytes != self.bitmap.len() || !trailing_bits_are_zero || self.side_nodes.next().is_some() {
            return Err(SparseMerkleError::InvalidMultiproof);
        }
        Ok(())
    }
}

fn root_from_proof<L: SparseMerkleLeaf>(
    leaves: &[ProofLeaf<'_, L>],
    depth: usize,
    proof: &mut ProofReader<'_>,
) -> Result<B256, SparseMerkleError> {
    if leaves.is_empty() {
        return proof.missing_subtree(empty_hash::<L>(depth));
    }
    if depth == L::Key::BITS {
        if leaves.len() != 1 {
            return Err(SparseMerkleError::InvalidMultiproof);
        }
        return Ok(leaves[0].hash(empty_hash::<L>(depth)));
    }

    let split = split(leaves, depth);
    let left = root_from_proof::<L>(&leaves[..split], depth + 1, proof)?;
    let right = root_from_proof::<L>(&leaves[split..], depth + 1, proof)?;
    Ok(Node {
        left: &left,
        right: &right,
    }
    .hash())
}

impl<L: SparseMerkleLeaf> SparseMerkleTree<L> {
    fn leaves(values: &[L]) -> Result<Vec<ProofLeaf<'_, L>>, SparseMerkleError> {
        let mut leaves: Vec<_> = values.iter().map(ProofLeaf::present).collect();
        leaves.sort_unstable_by_key(|leaf| leaf.key);
        if leaves.windows(2).any(|pair| pair[0].key == pair[1].key) {
            return Err(SparseMerkleError::InvalidMultiproof);
        }
        Ok(leaves)
    }

    fn ordered_leaves(values: &[L]) -> Result<Vec<ProofLeaf<'_, L>>, SparseMerkleError> {
        let leaves: Vec<_> = values.iter().map(ProofLeaf::present).collect();
        if leaves.windows(2).any(|pair| pair[0].key >= pair[1].key) {
            return Err(SparseMerkleError::InvalidMultiproof);
        }
        Ok(leaves)
    }

    fn select_leaves<'a>(
        keys: &[L::Key],
        leaves: &[ProofLeaf<'a, L>],
        reject_unselected: bool,
    ) -> Result<Vec<ProofLeaf<'a, L>>, SparseMerkleError> {
        let mut selected = Vec::with_capacity(keys.len());
        let mut leaf_index = 0;

        for key in keys {
            while leaves.get(leaf_index).is_some_and(|leaf| leaf.key < *key) {
                if reject_unselected {
                    return Err(SparseMerkleError::InvalidMultiproof);
                }
                leaf_index += 1;
            }
            if leaves.get(leaf_index).is_some_and(|leaf| leaf.key == *key) {
                selected.push(ProofLeaf::present(
                    leaves[leaf_index].value.expect("source leaf must have a value"),
                ));
                leaf_index += 1;
            } else {
                selected.push(ProofLeaf::absent(*key));
            }
        }
        if reject_unselected && leaf_index != leaves.len() {
            return Err(SparseMerkleError::InvalidMultiproof);
        }
        Ok(selected)
    }

    pub fn compute_root(values: &[L]) -> Result<B256, SparseMerkleError> {
        let leaves = Self::leaves(values)?;
        Ok(subtree_root::<L>(&leaves, 0))
    }

    pub fn build_multiproof(
        values: &[L],
        selected_keys: &[L::Key],
    ) -> Result<SparseMerkleMultiproof<L::Key>, SparseMerkleError> {
        let all = Self::leaves(values)?;
        if selected_keys.is_empty() {
            return Err(SparseMerkleError::InvalidMultiproof);
        }
        let mut leaf_keys = selected_keys.to_vec();
        leaf_keys.sort_unstable();
        if leaf_keys.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(SparseMerkleError::InvalidMultiproof);
        }
        let selected = Self::select_leaves(&leaf_keys, &all, false)?;

        let mut proof = ProofBuilder::new();
        build_proof::<L>(&all, &selected, 0, &mut proof);
        Ok(SparseMerkleMultiproof::new(leaf_keys, proof.bitmap, proof.side_nodes))
    }

    pub fn compute_root_from_proof(
        values: &[L],
        proof: &SparseMerkleMultiproof<L::Key>,
    ) -> Result<B256, SparseMerkleError> {
        proof.validate_keys()?;
        let supplied = Self::ordered_leaves(values)?;
        let leaves = Self::select_leaves(proof.leaf_keys(), &supplied, true)?;

        let mut reader = proof.reader();
        let root = root_from_proof::<L>(&leaves, 0, &mut reader)?;
        reader.finish()?;
        Ok(root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precomputed_empty_hashes_match_the_hashing_scheme() {
        let mut expected = B256::new(Sha256::digest(b"ZKCLOB_SMT_EMPTY_LEAF_V1").into());
        for actual in EMPTY_HASHES_BY_HEIGHT {
            assert_eq!(actual, expected);
            expected = Node {
                left: &expected,
                right: &expected,
            }
            .hash();
        }
    }

    #[derive(Clone)]
    struct TestLeaf {
        key: [u8; 1],
        value: u64,
    }

    impl Sha256Hash for TestLeaf {
        fn update_hash(&self, hasher: &mut Sha256) {
            hasher.update(self.key);
            hasher.update(self.value.to_be_bytes());
        }
    }

    impl DomainSha256Hash for TestLeaf {
        const DOMAIN: &'static [u8] = b"ZKCLOB_TEST_LEAF_V1";
    }

    impl SparseMerkleLeaf for TestLeaf {
        type Key = [u8; 1];

        fn key(&self) -> Self::Key {
            self.key
        }
    }

    type Tree = SparseMerkleTree<TestLeaf>;

    #[test]
    fn multiproof_reconstructs_root() {
        let leaves = vec![TestLeaf { key: [1], value: 10 }, TestLeaf { key: [200], value: 20 }];
        let proof = Tree::build_multiproof(&leaves, &[[1], [200]]).unwrap();

        assert_eq!(
            Tree::compute_root_from_proof(&leaves, &proof).unwrap(),
            Tree::compute_root(&leaves).unwrap()
        );
    }

    #[test]
    fn proof_reconstruction_rejects_unsorted_leaves() {
        let leaves = vec![TestLeaf { key: [1], value: 10 }, TestLeaf { key: [200], value: 20 }];
        let proof = Tree::build_multiproof(&leaves, &[[1], [200]]).unwrap();
        let unsorted = vec![leaves[1].clone(), leaves[0].clone()];

        assert_eq!(
            Tree::compute_root_from_proof(&unsorted, &proof),
            Err(SparseMerkleError::InvalidMultiproof)
        );
    }

    #[test]
    fn non_membership_proof_supports_insertion() {
        let old = vec![TestLeaf { key: [1], value: 10 }];
        let proof = Tree::build_multiproof(&old, &[[1], [2]]).unwrap();
        assert_eq!(
            Tree::compute_root_from_proof(&old, &proof).unwrap(),
            Tree::compute_root(&old).unwrap()
        );

        let new = vec![TestLeaf { key: [1], value: 10 }, TestLeaf { key: [2], value: 20 }];
        assert_eq!(
            Tree::compute_root_from_proof(&new, &proof).unwrap(),
            Tree::compute_root(&new).unwrap()
        );
    }
}
