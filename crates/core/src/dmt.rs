use alloy_primitives::B256;
use core::marker::PhantomData;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::hashing::{DomainSha256Hash, Sha256Hash};

#[derive(Deserialize, Serialize)]
pub struct DenseMerkleMultiproof {
    leaf_count: u32,
    leaf_indices: Vec<u32>,
    side_nodes: Vec<B256>,
}

impl DenseMerkleMultiproof {
    pub const fn new(leaf_count: u32, leaf_indices: Vec<u32>, side_nodes: Vec<B256>) -> Self {
        Self {
            leaf_count,
            leaf_indices,
            side_nodes,
        }
    }

    pub const fn leaf_count(&self) -> u32 {
        self.leaf_count
    }

    pub fn leaf_indices(&self) -> &[u32] {
        &self.leaf_indices
    }

    pub fn side_nodes(&self) -> &[B256] {
        &self.side_nodes
    }
}

struct EmptyLeaf;

impl Sha256Hash for EmptyLeaf {
    fn update_hash(&self, _hasher: &mut Sha256) {}
}

impl DomainSha256Hash for EmptyLeaf {
    const DOMAIN: &'static [u8] = b"ZKCLOB_DMT_EMPTY_LEAF_V1";
}

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
    const DOMAIN: &'static [u8] = b"ZKCLOB_DMT_NODE_V1";
}

struct Root<'a> {
    leaf_count: u32,
    tree_root: &'a B256,
}

impl Sha256Hash for Root<'_> {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.leaf_count.to_be_bytes());
        hasher.update(self.tree_root);
    }
}

impl DomainSha256Hash for Root<'_> {
    const DOMAIN: &'static [u8] = b"ZKCLOB_DMT_ROOT_V1";
}

pub(crate) enum DenseMerkleError {
    TooManyLeaves,
    InvalidMultiproof,
}

pub(crate) struct DenseMerkleTree<L>(PhantomData<L>);

impl<L: DomainSha256Hash> DenseMerkleTree<L> {
    pub(crate) fn compute_root(leaves: &[L]) -> B256 {
        let levels = build_levels(leaves);
        Root {
            leaf_count: u32::try_from(leaves.len()).expect("leaf count must fit in u32"),
            tree_root: &levels.last().expect("root level exists")[0],
        }
        .hash()
    }

    pub(crate) fn build_multiproof(
        leaves: &[L],
        leaf_indices: &[u32],
    ) -> Result<DenseMerkleMultiproof, DenseMerkleError> {
        let leaf_count =
            u32::try_from(leaves.len()).map_err(|_| DenseMerkleError::TooManyLeaves)?;
        if leaf_count == 0
            || leaf_indices.is_empty()
            || leaf_indices.windows(2).any(|pair| pair[0] >= pair[1])
            || leaf_indices
                .last()
                .is_some_and(|index| *index >= leaf_count)
        {
            return Err(DenseMerkleError::InvalidMultiproof);
        }

        let levels = build_levels(leaves);
        let mut side_nodes = Vec::new();
        build_side_nodes(
            &levels,
            leaf_indices,
            0,
            tree_width(leaves.len()),
            &mut side_nodes,
        );
        Ok(DenseMerkleMultiproof::new(
            leaf_count,
            leaf_indices.to_vec(),
            side_nodes,
        ))
    }

    pub(crate) fn compute_root_from_proof(
        leaves: &[L],
        proof: &DenseMerkleMultiproof,
    ) -> Result<B256, DenseMerkleError> {
        let indices = proof.leaf_indices();
        if leaves.len() != indices.len()
            || proof.leaf_count() == 0
            || indices.windows(2).any(|pair| pair[0] >= pair[1])
            || indices
                .last()
                .is_some_and(|index| *index >= proof.leaf_count())
        {
            return Err(DenseMerkleError::InvalidMultiproof);
        }

        let mut side_nodes = proof.side_nodes().iter().copied();
        let tree_root = tree_root_from_proof(
            leaves,
            indices,
            0,
            tree_width(proof.leaf_count() as usize),
            &mut side_nodes,
        )?;
        if side_nodes.next().is_some() {
            return Err(DenseMerkleError::InvalidMultiproof);
        }
        Ok(Root {
            leaf_count: proof.leaf_count(),
            tree_root: &tree_root,
        }
        .hash())
    }
}

fn tree_width(leaf_count: usize) -> usize {
    leaf_count.max(1).next_power_of_two()
}

fn build_levels<L: DomainSha256Hash>(leaves: &[L]) -> Vec<Vec<B256>> {
    let width = tree_width(leaves.len());
    let mut levels = Vec::new();
    let mut level: Vec<_> = leaves.iter().map(DomainSha256Hash::hash).collect();
    level.resize(width, EmptyLeaf.hash());
    levels.push(level);

    while levels.last().expect("leaf level exists").len() > 1 {
        let previous = levels.last().expect("previous level exists");
        let next = previous
            .chunks_exact(2)
            .map(|pair| {
                Node {
                    left: &pair[0],
                    right: &pair[1],
                }
                .hash()
            })
            .collect();
        levels.push(next);
    }
    levels
}

fn subtree_root(levels: &[Vec<B256>], start: usize, width: usize) -> B256 {
    let level = width.trailing_zeros() as usize;
    levels[level][start / width]
}

fn build_side_nodes(
    levels: &[Vec<B256>],
    selected: &[u32],
    start: usize,
    width: usize,
    side_nodes: &mut Vec<B256>,
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

fn tree_root_from_proof<L: DomainSha256Hash>(
    leaves: &[L],
    indices: &[u32],
    start: usize,
    width: usize,
    side_nodes: &mut impl Iterator<Item = B256>,
) -> Result<B256, DenseMerkleError> {
    if indices.is_empty() {
        return side_nodes.next().ok_or(DenseMerkleError::InvalidMultiproof);
    }
    if width == 1 {
        if leaves.len() != 1 || indices.len() != 1 || indices[0] as usize != start {
            return Err(DenseMerkleError::InvalidMultiproof);
        }
        return Ok(leaves[0].hash());
    }

    let midpoint = start + width / 2;
    let split = indices.partition_point(|index| (*index as usize) < midpoint);
    let left = tree_root_from_proof(
        &leaves[..split],
        &indices[..split],
        start,
        width / 2,
        side_nodes,
    )?;
    let right = tree_root_from_proof(
        &leaves[split..],
        &indices[split..],
        midpoint,
        width / 2,
        side_nodes,
    )?;
    Ok(Node {
        left: &left,
        right: &right,
    }
    .hash())
}
