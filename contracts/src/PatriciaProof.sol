// SPDX-License-Identifier: MIT
pragma solidity 0.8.35;

import {Math} from "openzeppelin-contracts/contracts/utils/math/Math.sol";

/// Verifies a single-account membership proof against the custom path-compressed Patricia
/// tree used for account state (see crates/core/src/trees/patricia.rs). Unlike the general
/// N-key multiproof reconstruction there, a single-leaf proof can be verified by folding side
/// nodes into the leaf one at a time, ordered by how deep each one splits off from the leaf's
/// key — every side node's position relative to a single leaf is fully determined by comparing
/// keys, so no recursive divide-and-conquer is needed. This equivalence is property-tested
/// against the general algorithm in patricia.rs before being relied on here.
library PatriciaProof {
    struct AssetBalance {
        address asset;
        uint128 available;
    }

    struct SideNode {
        bytes32 root;
        address minKey;
        address maxKey;
    }

    bytes private constant ACCOUNT_DOMAIN = "ZKCLOB_ACCOUNT_V1";
    bytes private constant LEAF_DOMAIN = "ZKCLOB_PATRICIA_LEAF_V1";
    bytes private constant BRANCH_DOMAIN = "ZKCLOB_PATRICIA_BRANCH_V1";
    bytes private constant ROOT_DOMAIN = "ZKCLOB_PATRICIA_ROOT_V1";

    error InvalidProof();

    /// Reconstructs the state root implied by `id`'s account leaf and its proof. Reverts
    /// `InvalidProof` on any structurally malformed input; a well-formed proof for the wrong
    /// leaf data or the wrong tree simply reconstructs a root that won't match the caller's
    /// expected value, which the caller (not this library) is responsible for checking.
    function verifyAccount(
        address id,
        uint64 nextNonce,
        AssetBalance[] memory balances,
        SideNode[] memory sideNodes
    ) internal pure returns (bytes32 root) {
        uint256 length = sideNodes.length;
        for (uint256 i; i < length; i++) {
            if (sideNodes[i].minKey > sideNodes[i].maxKey) revert InvalidProof();
            if (sideNodes[i].minKey <= id && id <= sideNodes[i].maxKey) revert InvalidProof();
            if (i > 0 && sideNodes[i - 1].maxKey >= sideNodes[i].minKey) revert InvalidProof();
        }

        // Derive each side node's depth (bit index, 0 = most significant, where its path
        // diverges from the leaf's key), then process deepest-first so each fold step
        // reconstructs exactly one branch along the leaf's root-to-leaf path.
        uint256[] memory order = new uint256[](length);
        uint256[] memory depths = new uint256[](length);
        for (uint256 i; i < length; i++) {
            order[i] = i;
            address boundary = sideNodes[i].maxKey < id ? sideNodes[i].maxKey : sideNodes[i].minKey;
            depths[i] = _differingBit(id, boundary);
        }
        // Selection sort by depth, descending. Proof sizes are effectively O(log n) accounts
        // (single digits to a few dozen for any real tree), so O(n^2) here is negligible.
        for (uint256 i; i < length; i++) {
            uint256 deepest = i;
            for (uint256 j = i + 1; j < length; j++) {
                if (depths[order[j]] > depths[order[deepest]]) deepest = j;
            }
            if (deepest != i) (order[i], order[deepest]) = (order[deepest], order[i]);
            if (i > 0 && depths[order[i]] == depths[order[i - 1]]) revert InvalidProof();
        }

        bytes32 currentRoot = _leafHash(id, nextNonce, balances);
        address currentMin = id;
        address currentMax = id;
        for (uint256 i; i < length; i++) {
            SideNode memory node = sideNodes[order[i]];
            if (node.maxKey < currentMin) {
                (currentRoot, currentMin, currentMax) =
                    _combine(node.root, node.minKey, node.maxKey, currentRoot, currentMin, currentMax);
            } else {
                (currentRoot, currentMin, currentMax) =
                    _combine(currentRoot, currentMin, currentMax, node.root, node.minKey, node.maxKey);
            }
        }

        root = sha256(abi.encodePacked(ROOT_DOMAIN, currentRoot, currentMin, currentMax));
    }

    function _combine(
        bytes32 leftRoot,
        address leftMin,
        address leftMax,
        bytes32 rightRoot,
        address rightMin,
        address rightMax
    ) private pure returns (bytes32 root, address min, address max) {
        if (leftMax >= rightMin) revert InvalidProof();
        uint256 depth = _differingBit(leftMax, rightMin);
        if (_bit(leftMin, depth) || _bit(leftMax, depth) || !_bit(rightMin, depth) || !_bit(rightMax, depth)) {
            revert InvalidProof();
        }

        root = sha256(
            abi.encodePacked(BRANCH_DOMAIN, uint64(depth), leftRoot, leftMin, leftMax, rightRoot, rightMin, rightMax)
        );
        min = leftMin;
        max = rightMax;
    }

    function _accountHash(address id, uint64 nextNonce, AssetBalance[] memory balances)
        private
        pure
        returns (bytes32)
    {
        bytes memory encoded = abi.encodePacked(ACCOUNT_DOMAIN, id, nextNonce, uint64(balances.length));
        for (uint256 i; i < balances.length; i++) {
            encoded = bytes.concat(encoded, abi.encodePacked(balances[i].asset, balances[i].available));
        }
        return sha256(encoded);
    }

    function _leafHash(address id, uint64 nextNonce, AssetBalance[] memory balances)
        private
        pure
        returns (bytes32)
    {
        return sha256(abi.encodePacked(LEAF_DOMAIN, id, _accountHash(id, nextNonce, balances)));
    }

    /// Bit `depth` of `key`, counting from the most significant bit (depth 0) through the
    /// least significant (depth 159) — matches crates/core/src/trees/patricia.rs's
    /// `PatriciaKey::bit`.
    function _bit(address key, uint256 depth) private pure returns (bool) {
        return (uint256(uint160(key)) >> (159 - depth)) & 1 == 1;
    }

    /// The bit index (0 = most significant of 160 bits) of the first differing bit between
    /// `a` and `b` — matches patricia.rs's `differing_bit`.
    function _differingBit(address a, address b) private pure returns (uint256) {
        uint256 diff = uint256(uint160(a) ^ uint160(b));
        if (diff == 0) revert InvalidProof();
        return 159 - Math.log2(diff);
    }
}
