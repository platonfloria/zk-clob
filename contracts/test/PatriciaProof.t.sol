// SPDX-License-Identifier: MIT
pragma solidity 0.8.35;

import {Test} from "forge-std/Test.sol";

import {PatriciaProof} from "../src/PatriciaProof.sol";

/// Fixture: a 4-account tree (ALICE, BOB, CAROL, TREASURY) with root and ALICE's proof
/// cross-computed against crates/core/src/trees/patricia.rs's real `Account`/`AccountId`
/// hashing (domain `ZKCLOB_ACCOUNT_V1`) via a one-off `cargo test -- --nocapture` run — not
/// derived independently in Solidity, so this test genuinely cross-validates the two
/// implementations rather than just checking Solidity against itself.
contract PatriciaProofTest is Test {
    address private constant ALICE = 0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf;
    address private constant BOB = 0x2B5AD5c4795c026514f8317c7a215E218DcCD6cF;
    address private constant CAROL = 0x1efF47bc3a10a45D4B230B5d10E37751FE6AA718;
    address private constant TREASURY = 0x6813Eb9362372EEF6200f3b1dbC3f819671cBA69;
    address private constant USDC = 0x0202020202020202020202020202020202020202;
    address private constant ETH = address(0);

    bytes32 private constant ROOT = 0x8ab24b83d11351dfa3b327e1fff8bd0ae47d53d9a2175810b1693a5d6d1527a3;

    function test_ValidProofReconstructsTheRoot() public pure {
        bytes32 root = PatriciaProof.verifyAccount(ALICE, 0, _aliceBalances(), _aliceSideNodes());

        assertEq(root, ROOT);
    }

    function test_TamperedLeafBalanceChangesTheRoot() public pure {
        PatriciaProof.AssetBalance[] memory balances = _aliceBalances();
        balances[0].available += 1;

        bytes32 root = PatriciaProof.verifyAccount(ALICE, 0, balances, _aliceSideNodes());

        assertNotEq(root, ROOT);
    }

    function test_TamperedSideNodeChangesTheRoot() public pure {
        PatriciaProof.SideNode[] memory sideNodes = _aliceSideNodes();
        sideNodes[0].root = bytes32(uint256(sideNodes[0].root) + 1);

        bytes32 root = PatriciaProof.verifyAccount(ALICE, 0, _aliceBalances(), sideNodes);

        assertNotEq(root, ROOT);
    }

    function test_TamperedNonceChangesTheRoot() public pure {
        bytes32 root = PatriciaProof.verifyAccount(ALICE, 1, _aliceBalances(), _aliceSideNodes());

        assertNotEq(root, ROOT);
    }

    function test_LeafKeyHiddenInASideNodeRangeReverts() public {
        PatriciaProof.SideNode[] memory sideNodes = new PatriciaProof.SideNode[](1);
        sideNodes[0] = PatriciaProof.SideNode({root: bytes32(uint256(1)), minKey: ALICE, maxKey: ALICE});

        vm.expectRevert(PatriciaProof.InvalidProof.selector);
        this.verifyAccount(ALICE, 0, _aliceBalances(), sideNodes);
    }

    function test_OverlappingSideNodeRangesReverts() public {
        PatriciaProof.SideNode[] memory sideNodes = new PatriciaProof.SideNode[](2);
        sideNodes[0] = PatriciaProof.SideNode({root: bytes32(uint256(1)), minKey: CAROL, maxKey: TREASURY});
        sideNodes[1] = PatriciaProof.SideNode({root: bytes32(uint256(2)), minKey: BOB, maxKey: TREASURY});

        vm.expectRevert(PatriciaProof.InvalidProof.selector);
        this.verifyAccount(ALICE, 0, _aliceBalances(), sideNodes);
    }

    function test_UnsortedSideNodesReverts() public {
        PatriciaProof.SideNode[] memory sideNodes = _aliceSideNodes();
        (sideNodes[0], sideNodes[1]) = (sideNodes[1], sideNodes[0]);

        vm.expectRevert(PatriciaProof.InvalidProof.selector);
        this.verifyAccount(ALICE, 0, _aliceBalances(), sideNodes);
    }

    /// `vm.expectRevert` needs an actual CALL frame boundary to intercept a revert; a direct
    /// call to `PatriciaProof.verifyAccount` (an `internal` function) gets inlined into the
    /// caller with no such boundary. This wrapper forces one via `this.verifyAccount(...)`.
    function verifyAccount(
        address id,
        uint64 nextNonce,
        PatriciaProof.AssetBalance[] memory balances,
        PatriciaProof.SideNode[] memory sideNodes
    ) external pure returns (bytes32) {
        return PatriciaProof.verifyAccount(id, nextNonce, balances, sideNodes);
    }

    function _aliceBalances() private pure returns (PatriciaProof.AssetBalance[] memory balances) {
        balances = new PatriciaProof.AssetBalance[](1);
        balances[0] = PatriciaProof.AssetBalance({asset: USDC, available: 100_000_000});
    }

    function _aliceSideNodes() private pure returns (PatriciaProof.SideNode[] memory sideNodes) {
        sideNodes = new PatriciaProof.SideNode[](2);
        sideNodes[0] = PatriciaProof.SideNode({
            root: 0x057b5a23b3d6f295826768cfa3d9fc753b463aff76d12cff7eb689017c62a9c7,
            minKey: CAROL,
            maxKey: BOB
        });
        sideNodes[1] = PatriciaProof.SideNode({
            root: 0xe02db1f29786859125015d310d1c184c359c2a7b1a8c74ae17bde133cd706a25,
            minKey: TREASURY,
            maxKey: TREASURY
        });
    }
}
