use alloy_primitives::B256;
use core::marker::PhantomData;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::hashing::{DomainSha256Hash, Sha256Hash};

/// A Patricia key whose [`Ord`] ordering is the same as the lexicographic
/// ordering produced by [`Self::bit`], from bit zero through `BITS - 1`.
pub trait PatriciaKey: Copy + Ord + Sha256Hash {
    const BITS: usize;

    fn bit(&self, index: usize) -> bool;
}

impl<const N: usize> Sha256Hash for [u8; N] {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self);
    }
}

impl<const N: usize> PatriciaKey for [u8; N] {
    const BITS: usize = N * 8;

    fn bit(&self, index: usize) -> bool {
        self[index / 8] & (1 << (7 - index % 8)) != 0
    }
}

pub trait PatriciaLeaf: DomainSha256Hash {
    type Key: PatriciaKey;

    fn key(&self) -> Self::Key;
}

#[derive(Debug, Eq, PartialEq)]
pub enum PatriciaError {
    InvalidMultiproof,
}

/// Root and key range of one canonical, unexpanded Patricia subtree.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PatriciaSubtree<K> {
    root: B256,
    min_key: K,
    max_key: K,
}

impl<K: Copy> PatriciaSubtree<K> {
    pub const fn new(root: B256, min_key: K, max_key: K) -> Self {
        Self { root, min_key, max_key }
    }

    pub const fn root(&self) -> &B256 {
        &self.root
    }

    pub const fn min_key(&self) -> K {
        self.min_key
    }

    pub const fn max_key(&self) -> K {
        self.max_key
    }
}

/// Selected keys plus the canonical subtrees which contain no selected key.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PatriciaMultiproof<K> {
    leaf_keys: Vec<K>,
    side_nodes: Vec<PatriciaSubtree<K>>,
}

impl<K> PatriciaMultiproof<K> {
    pub const fn new(leaf_keys: Vec<K>, side_nodes: Vec<PatriciaSubtree<K>>) -> Self {
        Self { leaf_keys, side_nodes }
    }

    pub fn leaf_keys(&self) -> &[K] {
        &self.leaf_keys
    }

    pub fn side_nodes(&self) -> &[PatriciaSubtree<K>] {
        &self.side_nodes
    }
}

#[derive(Clone, Copy)]
struct Commitment<K> {
    root: B256,
    min: K,
    max: K,
}

impl<K: Copy> From<&PatriciaSubtree<K>> for Commitment<K> {
    fn from(value: &PatriciaSubtree<K>) -> Self {
        Self {
            root: value.root,
            min: value.min_key,
            max: value.max_key,
        }
    }
}

impl<K: Copy> From<Commitment<K>> for PatriciaSubtree<K> {
    fn from(value: Commitment<K>) -> Self {
        Self::new(value.root, value.min, value.max)
    }
}

enum Node<K> {
    Empty,
    Leaf(Commitment<K>),
    Branch {
        commitment: Commitment<K>,
        left: Box<Self>,
        right: Box<Self>,
    },
}

impl<K: Copy> Node<K> {
    const fn commitment(&self) -> Option<Commitment<K>> {
        match self {
            Self::Empty => None,
            Self::Leaf(value) | Self::Branch { commitment: value, .. } => Some(*value),
        }
    }
}

struct Empty;

impl Sha256Hash for Empty {
    fn update_hash(&self, _hasher: &mut Sha256) {}
}

impl DomainSha256Hash for Empty {
    const DOMAIN: &'static [u8] = b"ZKCLOB_PATRICIA_EMPTY_V1";
}

struct Leaf<'a, L>(&'a L);

impl<L: PatriciaLeaf> Sha256Hash for Leaf<'_, L> {
    fn update_hash(&self, hasher: &mut Sha256) {
        self.0.key().update_hash(hasher);
        self.0.hash().update_hash(hasher);
    }
}

impl<L: PatriciaLeaf> DomainSha256Hash for Leaf<'_, L> {
    const DOMAIN: &'static [u8] = b"ZKCLOB_PATRICIA_LEAF_V1";
}

struct Branch<K> {
    depth: usize,
    left: Commitment<K>,
    right: Commitment<K>,
}

impl<K: PatriciaKey> Sha256Hash for Branch<K> {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(
            u64::try_from(self.depth)
                .expect("Patricia depth must fit in u64")
                .to_be_bytes(),
        );
        for node in [self.left, self.right] {
            node.root.update_hash(hasher);
            node.min.update_hash(hasher);
            node.max.update_hash(hasher);
        }
    }
}

impl<K: PatriciaKey> DomainSha256Hash for Branch<K> {
    const DOMAIN: &'static [u8] = b"ZKCLOB_PATRICIA_BRANCH_V1";
}

struct Root<K>(Commitment<K>);

impl<K: PatriciaKey> Sha256Hash for Root<K> {
    fn update_hash(&self, hasher: &mut Sha256) {
        self.0.root.update_hash(hasher);
        self.0.min.update_hash(hasher);
        self.0.max.update_hash(hasher);
    }
}

impl<K: PatriciaKey> DomainSha256Hash for Root<K> {
    const DOMAIN: &'static [u8] = b"ZKCLOB_PATRICIA_ROOT_V1";
}

fn differing_bit<K: PatriciaKey>(left: K, right: K) -> Option<usize> {
    (0..K::BITS).find(|bit| left.bit(*bit) != right.bit(*bit))
}

fn leaf<L: PatriciaLeaf>(value: &L) -> Commitment<L::Key> {
    let key = value.key();
    Commitment {
        root: Leaf(value).hash(),
        min: key,
        max: key,
    }
}

fn branch<K: PatriciaKey>(left: Commitment<K>, right: Commitment<K>) -> Result<Commitment<K>, PatriciaError> {
    if left.max >= right.min {
        return Err(PatriciaError::InvalidMultiproof);
    }
    let depth = differing_bit(left.max, right.min).ok_or(PatriciaError::InvalidMultiproof)?;
    if left.min.bit(depth) || left.max.bit(depth) || !right.min.bit(depth) || !right.max.bit(depth) {
        return Err(PatriciaError::InvalidMultiproof);
    }
    Ok(Commitment {
        root: Branch { depth, left, right }.hash(),
        min: left.min,
        max: right.max,
    })
}

fn build<L: PatriciaLeaf>(values: &[&L]) -> Result<Node<L::Key>, PatriciaError> {
    match values {
        [] => Ok(Node::Empty),
        [value] => Ok(Node::Leaf(leaf(*value))),
        values => {
            let depth = differing_bit(values[0].key(), values[values.len() - 1].key())
                .ok_or(PatriciaError::InvalidMultiproof)?;
            let split = values.partition_point(|value| !value.key().bit(depth));
            if split == 0 || split == values.len() {
                return Err(PatriciaError::InvalidMultiproof);
            }
            let left = build(&values[..split])?;
            let right = build(&values[split..])?;
            let commitment = branch(
                left.commitment().expect("left branch must exist"),
                right.commitment().expect("right branch must exist"),
            )?;
            Ok(Node::Branch {
                commitment,
                left: Box::new(left),
                right: Box::new(right),
            })
        }
    }
}

fn selected_in<K: PatriciaKey>(keys: &[K], node: Commitment<K>) -> bool {
    let index = keys.partition_point(|key| *key < node.min);
    keys.get(index).is_some_and(|key| *key <= node.max)
}

fn side_nodes<K: PatriciaKey>(node: &Node<K>, keys: &[K], output: &mut Vec<PatriciaSubtree<K>>) {
    let Some(commitment) = node.commitment() else {
        return;
    };
    if let Node::Branch { left, right, .. } = node {
        let depth = differing_bit(
            left.commitment().expect("left branch must exist").max,
            right.commitment().expect("right branch must exist").min,
        )
        .expect("branch children must differ");
        let affected = selected_in(keys, commitment)
            || keys.iter().any(|key| {
                let boundary = if *key < commitment.min {
                    commitment.min
                } else {
                    commitment.max
                };
                differing_bit(*key, boundary).is_some_and(|difference| difference > depth)
            });
        if affected {
            side_nodes(left, keys, output);
            side_nodes(right, keys, output);
            return;
        }
    } else if selected_in(keys, commitment) {
        return;
    }
    output.push(commitment.into());
}

fn combine<K: PatriciaKey>(nodes: &[Commitment<K>]) -> Result<Option<Commitment<K>>, PatriciaError> {
    match nodes {
        [] => Ok(None),
        [node] => Ok(Some(*node)),
        nodes => {
            let depth =
                differing_bit(nodes[0].min, nodes[nodes.len() - 1].max).ok_or(PatriciaError::InvalidMultiproof)?;
            let split = nodes.partition_point(|node| !node.max.bit(depth));
            if split == 0
                || split == nodes.len()
                || nodes[..split].iter().any(|node| node.min.bit(depth))
                || nodes[split..].iter().any(|node| !node.min.bit(depth))
            {
                return Err(PatriciaError::InvalidMultiproof);
            }
            Ok(Some(branch(
                combine(&nodes[..split])?.ok_or(PatriciaError::InvalidMultiproof)?,
                combine(&nodes[split..])?.ok_or(PatriciaError::InvalidMultiproof)?,
            )?))
        }
    }
}

fn finalize<K: PatriciaKey>(node: Option<Commitment<K>>) -> B256 {
    node.map_or_else(|| Empty.hash(), |node| Root(node).hash())
}

pub struct PatriciaMerkleTree<L>(PhantomData<L>);

impl<L: PatriciaLeaf> PatriciaMerkleTree<L> {
    fn sorted(values: &[L]) -> Result<Vec<&L>, PatriciaError> {
        let mut values: Vec<_> = values.iter().collect();
        values.sort_unstable_by_key(|value| value.key());
        if values.windows(2).any(|pair| pair[0].key() == pair[1].key()) {
            return Err(PatriciaError::InvalidMultiproof);
        }
        Ok(values)
    }

    pub fn compute_root(values: &[L]) -> Result<B256, PatriciaError> {
        Ok(finalize(build(&Self::sorted(values)?)?.commitment()))
    }

    pub fn build_multiproof(
        values: &[L],
        selected_keys: &[L::Key],
    ) -> Result<PatriciaMultiproof<L::Key>, PatriciaError> {
        if selected_keys.is_empty() {
            return Err(PatriciaError::InvalidMultiproof);
        }
        let mut keys = selected_keys.to_vec();
        keys.sort_unstable();
        if keys.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(PatriciaError::InvalidMultiproof);
        }
        let tree = build(&Self::sorted(values)?)?;
        let mut proof_nodes = Vec::new();
        side_nodes(&tree, &keys, &mut proof_nodes);
        Ok(PatriciaMultiproof::new(keys, proof_nodes))
    }

    pub fn compute_root_from_proof(values: &[L], proof: &PatriciaMultiproof<L::Key>) -> Result<B256, PatriciaError> {
        if proof.leaf_keys.is_empty()
            || proof.leaf_keys.windows(2).any(|pair| pair[0] >= pair[1])
            || proof.side_nodes.iter().any(|node| node.min_key > node.max_key)
            || proof
                .side_nodes
                .windows(2)
                .any(|pair| pair[0].max_key >= pair[1].min_key)
        {
            return Err(PatriciaError::InvalidMultiproof);
        }
        let values = Self::sorted(values)?;
        if values
            .iter()
            .any(|value| proof.leaf_keys.binary_search(&value.key()).is_err())
            || proof.side_nodes.iter().any(|node| {
                let index = proof.leaf_keys.partition_point(|key| *key < node.min_key);
                proof.leaf_keys.get(index).is_some_and(|key| *key <= node.max_key)
            })
        {
            return Err(PatriciaError::InvalidMultiproof);
        }

        let leaves: Vec<_> = values.iter().map(|value| leaf(*value)).collect();
        let mut nodes = Vec::with_capacity(leaves.len() + proof.side_nodes.len());
        let (mut leaf_index, mut side_index) = (0, 0);
        while leaf_index < leaves.len() || side_index < proof.side_nodes.len() {
            let use_leaf = side_index == proof.side_nodes.len()
                || leaf_index < leaves.len() && leaves[leaf_index].min < proof.side_nodes[side_index].min_key;
            if use_leaf {
                nodes.push(leaves[leaf_index]);
                leaf_index += 1;
            } else {
                nodes.push((&proof.side_nodes[side_index]).into());
                side_index += 1;
            }
        }
        if nodes.windows(2).any(|pair| pair[0].max >= pair[1].min) {
            return Err(PatriciaError::InvalidMultiproof);
        }
        Ok(finalize(combine(&nodes)?))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use proptest::collection::{btree_map, btree_set};
    use proptest::prelude::*;

    use super::*;

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
        const DOMAIN: &'static [u8] = b"ZKCLOB_PATRICIA_TEST_V1";
    }

    impl PatriciaLeaf for TestLeaf {
        type Key = [u8; 1];

        fn key(&self) -> Self::Key {
            self.key
        }
    }

    type Tree = PatriciaMerkleTree<TestLeaf>;

    fn leaves() -> Vec<TestLeaf> {
        vec![
            TestLeaf { key: [1], value: 10 },
            TestLeaf { key: [100], value: 20 },
            TestLeaf { key: [200], value: 30 },
        ]
    }

    #[test]
    fn multiproof_reconstructs_root() {
        let leaves = leaves();
        let proof = Tree::build_multiproof(&leaves, &[[1], [200]]).unwrap();
        assert_eq!(
            Tree::compute_root_from_proof(&[leaves[0].clone(), leaves[2].clone()], &proof).unwrap(),
            Tree::compute_root(&leaves).unwrap()
        );
    }

    #[test]
    fn non_membership_proof_supports_insertion() {
        let old = leaves();
        let proof = Tree::build_multiproof(&old, &[[50]]).unwrap();
        assert_eq!(
            Tree::compute_root_from_proof(&[], &proof).unwrap(),
            Tree::compute_root(&old).unwrap()
        );
        let inserted = TestLeaf { key: [50], value: 40 };
        let mut new = old.clone();
        new.push(inserted.clone());
        assert_eq!(
            Tree::compute_root_from_proof(&[inserted], &proof).unwrap(),
            Tree::compute_root(&new).unwrap()
        );
    }

    #[test]
    fn empty_tree_proof_supports_first_insertion() {
        let inserted = TestLeaf { key: [50], value: 40 };
        let proof = Tree::build_multiproof(&[], &[[50]]).unwrap();

        assert_eq!(
            Tree::compute_root_from_proof(&[], &proof).unwrap(),
            Tree::compute_root(&[]).unwrap()
        );
        assert_eq!(
            Tree::compute_root_from_proof(&[inserted.clone()], &proof).unwrap(),
            Tree::compute_root(&[inserted]).unwrap()
        );
    }

    #[test]
    fn update_reconstructs_new_root() {
        let old = leaves();
        let proof = Tree::build_multiproof(&old, &[[100]]).unwrap();
        let updated = TestLeaf { key: [100], value: 21 };
        assert_eq!(
            Tree::compute_root_from_proof(&[updated.clone()], &proof).unwrap(),
            Tree::compute_root(&[old[0].clone(), updated, old[2].clone()]).unwrap()
        );
    }

    #[test]
    fn selected_key_cannot_be_hidden_in_side_node() {
        let leaves = leaves();
        let mut proof = Tree::build_multiproof(&leaves, &[[1]]).unwrap();
        proof.leaf_keys.push([100]);
        assert_eq!(
            Tree::compute_root_from_proof(&[leaves[0].clone()], &proof),
            Err(PatriciaError::InvalidMultiproof)
        );
    }

    #[test]
    fn rejects_overlapping_side_node_ranges() {
        let proof = PatriciaMultiproof::new(
            vec![[200]],
            vec![
                PatriciaSubtree::new(B256::ZERO, [1], [100]),
                PatriciaSubtree::new(B256::ZERO, [100], [150]),
            ],
        );

        assert_eq!(
            Tree::compute_root_from_proof(&[], &proof),
            Err(PatriciaError::InvalidMultiproof)
        );
    }

    #[test]
    fn tampering_with_a_side_node_changes_the_root() {
        let leaves = leaves();
        let mut proof = Tree::build_multiproof(&leaves, &[[1]]).unwrap();
        proof.side_nodes[0].root = B256::with_last_byte(1);

        assert_ne!(
            Tree::compute_root_from_proof(&[leaves[0].clone()], &proof).unwrap(),
            Tree::compute_root(&leaves).unwrap()
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1_000))]

        #[test]
        fn proofs_reconstruct_old_and_updated_roots(
            old_values in btree_map(any::<u8>(), any::<u64>(), 0..20),
            selected in btree_set(any::<u8>(), 1..12),
            replacement_seed in any::<u64>(),
        ) {
            let old: Vec<_> = old_values
                .iter()
                .map(|(key, value)| TestLeaf { key: [*key], value: *value })
                .collect();
            let selected_keys: Vec<_> = selected.iter().map(|key| [*key]).collect();
            let proof = Tree::build_multiproof(&old, &selected_keys).unwrap();
            let old_selected: Vec<_> = old
                .iter()
                .filter(|leaf| selected.contains(&leaf.key[0]))
                .cloned()
                .collect();

            prop_assert_eq!(
                Tree::compute_root_from_proof(&old_selected, &proof).unwrap(),
                Tree::compute_root(&old).unwrap()
            );

            let mut new_values: BTreeMap<_, _> = old_values;
            for key in &selected {
                let old_value = new_values.get(key).copied().unwrap_or_default();
                new_values.insert(*key, old_value.wrapping_add(replacement_seed).wrapping_add(1));
            }
            let new: Vec<_> = new_values
                .iter()
                .map(|(key, value)| TestLeaf { key: [*key], value: *value })
                .collect();
            let new_selected: Vec<_> = new
                .iter()
                .filter(|leaf| selected.contains(&leaf.key[0]))
                .cloned()
                .collect();

            prop_assert_eq!(
                Tree::compute_root_from_proof(&new_selected, &proof).unwrap(),
                Tree::compute_root(&new).unwrap()
            );

            let retained: Vec<_> = old
                .iter()
                .filter(|leaf| !selected.contains(&leaf.key[0]))
                .cloned()
                .collect();
            prop_assert_eq!(
                Tree::compute_root_from_proof(&[], &proof).unwrap(),
                Tree::compute_root(&retained).unwrap()
            );
        }

        #[test]
        fn root_is_independent_of_input_order(
            values in btree_map(any::<u8>(), any::<u64>(), 0..20),
        ) {
            let ordered: Vec<_> = values
                .iter()
                .map(|(key, value)| TestLeaf { key: [*key], value: *value })
                .collect();
            let mut reversed = ordered.clone();
            reversed.reverse();

            prop_assert_eq!(
                Tree::compute_root(&ordered).unwrap(),
                Tree::compute_root(&reversed).unwrap()
            );
        }
    }
}
