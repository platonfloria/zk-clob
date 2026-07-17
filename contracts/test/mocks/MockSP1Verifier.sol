// SPDX-License-Identifier: MIT
pragma solidity 0.8.20;

import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";

contract MockSP1Verifier is ISP1Verifier {
    error ProofRejected();

    bool public rejectProof;

    function setRejectProof(bool reject) external {
        rejectProof = reject;
    }

    function verifyProof(bytes32, bytes calldata, bytes calldata) external view {
        if (rejectProof) revert ProofRejected();
    }
}
