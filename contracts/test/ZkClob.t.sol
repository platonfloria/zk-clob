// SPDX-License-Identifier: MIT
pragma solidity 0.8.20;

import {Test} from "forge-std/Test.sol";

import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";
import {SP1Verifier as SP1Groth16Verifier} from "@sp1-contracts/v6.1.0/SP1VerifierGroth16.sol";

import {IZkClob} from "../src/IZkClob.sol";
import {ZkClob} from "../src/ZkClob.sol";
import {MockSP1Verifier} from "./mocks/MockSP1Verifier.sol";

contract ZkClobTest is Test {
    string private constant PUBLIC_VALUES_PATH = "../testdata/public-values.bin";
    string private constant PROOF_PATH = "../testdata/proof.bin";
    string private constant PROGRAM_VKEY_PATH = "../testdata/program-vkey.txt";

    bytes private publicValues;
    bytes private proof;
    bytes32 private programVKey;
    IZkClob.PublicOutput private output;

    MockSP1Verifier private mockVerifier;
    ZkClob private exchange;

    event BatchSettled(
        uint64 indexed batchId,
        bytes32 indexed oldStateRoot,
        bytes32 indexed newStateRoot,
        bytes32 batchHash,
        bytes32 tradesHash
    );

    function setUp() public {
        publicValues = vm.readFileBinary(PUBLIC_VALUES_PATH);
        proof = vm.readFileBinary(PROOF_PATH);
        programVKey = vm.parseBytes32(vm.readFile(PROGRAM_VKEY_PATH));
        output = abi.decode(publicValues, (IZkClob.PublicOutput));

        vm.chainId(output.metadata.chainId);

        mockVerifier = new MockSP1Verifier();
        exchange = _deploy(ISP1Verifier(address(mockVerifier)));
    }

    function test_SettleUpdatesStateUsingFixture() public {
        vm.expectEmit(true, true, true, true);
        emit BatchSettled(
            output.metadata.batchId, output.oldStateRoot, output.newStateRoot, output.batchHash, output.tradesHash
        );

        exchange.settle(publicValues, proof);

        assertEq(exchange.stateRoot(), output.newStateRoot);
        assertEq(exchange.nextBatchId(), output.metadata.batchId + 1);
    }

    function test_RealGroth16ProofFromTestdataVerifies() public {
        SP1Groth16Verifier verifier = new SP1Groth16Verifier();
        ZkClob realExchange = _deploy(ISP1Verifier(address(verifier)));

        realExchange.settle(publicValues, proof);

        assertEq(realExchange.stateRoot(), output.newStateRoot);
        assertEq(realExchange.nextBatchId(), output.metadata.batchId + 1);
    }

    function test_ReplayReverts() public {
        exchange.settle(publicValues, proof);

        vm.expectRevert(
            abi.encodeWithSelector(IZkClob.WrongBatchId.selector, output.metadata.batchId + 1, output.metadata.batchId)
        );
        exchange.settle(publicValues, proof);
    }

    function test_VerifierRejectionLeavesStateUnchanged() public {
        mockVerifier.setRejectProof(true);

        vm.expectRevert(MockSP1Verifier.ProofRejected.selector);
        exchange.settle(publicValues, proof);

        assertEq(exchange.stateRoot(), output.oldStateRoot);
        assertEq(exchange.nextBatchId(), output.metadata.batchId);
    }

    function test_InvalidPublicValuesLengthReverts() public {
        bytes memory truncated = new bytes(publicValues.length - 1);

        vm.expectRevert(abi.encodeWithSelector(IZkClob.InvalidPublicValuesLength.selector, truncated.length));
        exchange.settle(truncated, proof);
    }

    function test_WrongProtocolVersionReverts() public {
        IZkClob.PublicOutput memory changed = output;
        changed.metadata.protocolVersion++;

        vm.expectRevert(
            abi.encodeWithSelector(
                IZkClob.WrongProtocolVersion.selector, output.metadata.protocolVersion, changed.metadata.protocolVersion
            )
        );
        exchange.settle(abi.encode(changed), proof);
    }

    function test_WrongChainReverts() public {
        uint64 actualChainId = output.metadata.chainId + 1;
        vm.chainId(actualChainId);

        vm.expectRevert(
            abi.encodeWithSelector(IZkClob.WrongChain.selector, uint256(actualChainId), output.metadata.chainId)
        );
        exchange.settle(publicValues, proof);
    }

    function test_WrongExchangeReverts() public {
        IZkClob.PublicOutput memory changed = output;
        changed.metadata.exchangeId = bytes32(uint256(output.metadata.exchangeId) + 1);

        vm.expectRevert(
            abi.encodeWithSelector(
                IZkClob.WrongExchange.selector, output.metadata.exchangeId, changed.metadata.exchangeId
            )
        );
        exchange.settle(abi.encode(changed), proof);
    }

    function test_WrongConfigReverts() public {
        IZkClob.PublicOutput memory changed = output;
        changed.configHash = bytes32(uint256(output.configHash) + 1);

        vm.expectRevert(abi.encodeWithSelector(IZkClob.WrongConfig.selector, output.configHash, changed.configHash));
        exchange.settle(abi.encode(changed), proof);
    }

    function test_WrongBatchIdReverts() public {
        IZkClob.PublicOutput memory changed = output;
        changed.metadata.batchId++;

        vm.expectRevert(
            abi.encodeWithSelector(IZkClob.WrongBatchId.selector, output.metadata.batchId, changed.metadata.batchId)
        );
        exchange.settle(abi.encode(changed), proof);
    }

    function test_StaleStateRootReverts() public {
        IZkClob.PublicOutput memory changed = output;
        changed.oldStateRoot = bytes32(uint256(output.oldStateRoot) + 1);

        vm.expectRevert(
            abi.encodeWithSelector(IZkClob.StaleStateRoot.selector, output.oldStateRoot, changed.oldStateRoot)
        );
        exchange.settle(abi.encode(changed), proof);
    }

    function test_ZeroVerifierReverts() public {
        vm.expectRevert(IZkClob.ZeroVerifier.selector);
        new ZkClob(
            ISP1Verifier(address(0)),
            programVKey,
            output.metadata.exchangeId,
            output.configHash,
            output.metadata.protocolVersion,
            output.oldStateRoot,
            output.metadata.batchId
        );
    }

    function _deploy(ISP1Verifier verifier) private returns (ZkClob) {
        return new ZkClob(
            verifier,
            programVKey,
            output.metadata.exchangeId,
            output.configHash,
            output.metadata.protocolVersion,
            output.oldStateRoot,
            output.metadata.batchId
        );
    }
}
