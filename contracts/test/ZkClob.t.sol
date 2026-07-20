// SPDX-License-Identifier: MIT
pragma solidity 0.8.35;

import {Test} from "forge-std/Test.sol";

import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";
import {SP1Verifier as SP1Groth16Verifier} from "@sp1-contracts/v6.1.0/SP1VerifierGroth16.sol";

import {IZkClob} from "../src/IZkClob.sol";
import {ZkClob} from "../src/ZkClob.sol";
import {MockSP1Verifier} from "./mocks/MockSP1Verifier.sol";
import {MockERC20} from "./mocks/MockERC20.sol";

contract ZkClobTest is Test {
    address private constant FIXTURE_ACCOUNT = 0x0101010101010101010101010101010101010101;
    uint128 private constant FIXTURE_DEPOSIT_AMOUNT = 1 ether;

    string private constant PUBLIC_VALUES_PATH = "../testdata/public-values.bin";
    string private constant PROOF_PATH = "../testdata/proof.bin";
    string private constant PROGRAM_VKEY_PATH = "../testdata/program-vkey.txt";
    string private constant OLD_PROGRAM_VKEY_PATH = "../testdata/old-program-vkey.txt";

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

    event DepositQueued(uint64 indexed depositId, address indexed account, address indexed asset, uint128 amount);

    function setUp() public {
        publicValues = vm.readFileBinary(PUBLIC_VALUES_PATH);
        proof = vm.readFileBinary(PROOF_PATH);
        programVKey = vm.parseBytes32(vm.readFile(PROGRAM_VKEY_PATH));
        output = abi.decode(publicValues, (IZkClob.PublicOutput));

        vm.chainId(output.metadata.chainId);

        mockVerifier = new MockSP1Verifier();
        exchange = _deploy(ISP1Verifier(address(mockVerifier)));
        _queueFixtureDeposit(exchange);
    }

    function test_SettleUpdatesStateUsingFixture() public {
        vm.expectEmit(true, true, true, true);
        emit BatchSettled(
            output.metadata.batchId, output.oldStateRoot, output.newStateRoot, output.batchHash, output.tradesHash
        );

        exchange.settle(publicValues, proof);

        assertEq(exchange.stateRoot(), output.newStateRoot);
        assertEq(exchange.nextBatchId(), output.metadata.batchId + 1);
        assertEq(exchange.nextUnprocessedDeposit(), output.newDepositCursor);
    }

    function test_DepositEthLocksFundsAndQueuesMessage() public {
        address user = makeAddr("depositor");
        uint128 amount = 2 ether;
        vm.deal(user, amount);

        vm.expectEmit(true, true, true, true);
        emit DepositQueued(1, user, address(0), amount);
        vm.prank(user);
        uint64 depositId = exchange.deposit{value: amount}();

        assertEq(depositId, 1);
        assertEq(exchange.nextDepositId(), 2);
        assertEq(address(exchange).balance, FIXTURE_DEPOSIT_AMOUNT + amount);
        (address account, address asset, uint128 queuedAmount) = exchange.deposits(depositId);
        assertEq(account, user);
        assertEq(asset, address(0));
        assertEq(queuedAmount, amount);
    }

    function test_DepositErc20LocksExactFundsAndUsesNextId() public {
        address user = makeAddr("token-depositor");
        uint128 amount = 1_000_000;
        MockERC20 token = new MockERC20();
        token.mint(user, amount);
        vm.deal(user, 1 wei);

        vm.prank(user);
        exchange.deposit{value: 1 wei}();

        vm.startPrank(user);
        token.approve(address(exchange), amount);
        vm.expectEmit(true, true, true, true);
        emit DepositQueued(2, user, address(token), amount);
        uint64 depositId = exchange.deposit(address(token), amount);
        vm.stopPrank();

        assertEq(depositId, 2);
        assertEq(exchange.nextDepositId(), 3);
        assertEq(token.balanceOf(address(exchange)), amount);
        (address account, address asset, uint128 queuedAmount) = exchange.deposits(depositId);
        assertEq(account, user);
        assertEq(asset, address(token));
        assertEq(queuedAmount, amount);
    }

    function test_ZeroDepositRevertsWithoutAdvancingQueue() public {
        vm.expectRevert(IZkClob.ZeroDepositAmount.selector);
        exchange.deposit();

        assertEq(exchange.nextDepositId(), 1);
    }

    function test_Erc20DepositRejectsNativeValue() public {
        MockERC20 token = new MockERC20();

        vm.expectRevert(abi.encodeWithSelector(IZkClob.UnexpectedNativeValue.selector, 1 wei));
        exchange.deposit{value: 1 wei}(address(token), 1);

        assertEq(exchange.nextDepositId(), 1);
    }

    function test_RealGroth16ProofFromTestdataVerifies() public {
        SP1Groth16Verifier verifier = new SP1Groth16Verifier();
        ZkClob realExchange = _deploy(ISP1Verifier(address(verifier)));
        _queueFixtureDeposit(realExchange);

        realExchange.settle(publicValues, proof);

        assertEq(realExchange.stateRoot(), output.newStateRoot);
        assertEq(realExchange.nextBatchId(), output.metadata.batchId + 1);
        assertEq(realExchange.nextUnprocessedDeposit(), output.newDepositCursor);
    }

    function test_SettleConsumesQueuedDepositPrefix() public {
        address alice = makeAddr("alice");
        address bob = makeAddr("bob");
        vm.deal(alice, 1 ether);
        vm.deal(bob, 2 ether);
        vm.prank(alice);
        exchange.deposit{value: 1 ether}();
        vm.prank(bob);
        exchange.deposit{value: 2 ether}();

        IZkClob.PublicOutput memory changed = output;
        changed.newDepositCursor = 3;
        changed.consumedDepositsHash = _threeNativeDepositsHash(alice, bob);

        exchange.settle(abi.encode(changed), proof);

        assertEq(exchange.nextUnprocessedDeposit(), 3);
        assertEq(exchange.stateRoot(), changed.newStateRoot);
    }

    function test_WrongOldDepositCursorReverts() public {
        IZkClob.PublicOutput memory changed = output;
        changed.oldDepositCursor = 2;
        changed.newDepositCursor = 2;

        vm.expectRevert(abi.encodeWithSelector(IZkClob.WrongDepositCursor.selector, 0, 2));
        exchange.settle(abi.encode(changed), proof);
    }

    function test_DepositCursorCannotAdvancePastQueue() public {
        IZkClob.PublicOutput memory changed = output;
        changed.newDepositCursor = 2;

        vm.expectRevert(abi.encodeWithSelector(IZkClob.InvalidDepositCursorAdvance.selector, 0, 2, 1));
        exchange.settle(abi.encode(changed), proof);
    }

    function test_WrongConsumedDepositsHashReverts() public {
        IZkClob.PublicOutput memory changed = output;
        changed.consumedDepositsHash = bytes32(uint256(changed.consumedDepositsHash) + 1);
        bytes32 expected = sha256(
            abi.encodePacked(
                "ZKCLOB_DEPOSITS_V1", uint64(1), uint64(0), FIXTURE_ACCOUNT, bytes32(0), FIXTURE_DEPOSIT_AMOUNT
            )
        );

        vm.expectRevert(
            abi.encodeWithSelector(
                IZkClob.ConsumedDepositsHashMismatch.selector, expected, changed.consumedDepositsHash
            )
        );
        exchange.settle(abi.encode(changed), proof);
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

    function test_MalformedPublicValuesRevert() public {
        bytes memory malformed = publicValues;
        // A uint32 occupies the low four bytes of its ABI word. Setting a high
        // padding byte makes the otherwise correctly sized encoding invalid.
        malformed[0] = 0x01;

        vm.expectRevert();
        exchange.settle(malformed, proof);
    }

    function test_ProofForDifferentProgramVKeyReverts() public {
        bytes32 oldProgramVKey = vm.parseBytes32(vm.readFile(OLD_PROGRAM_VKEY_PATH));
        SP1Groth16Verifier verifier = new SP1Groth16Verifier();

        vm.expectRevert();
        verifier.verifyProof(oldProgramVKey, publicValues, proof);
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

    function _queueFixtureDeposit(ZkClob target) private {
        vm.deal(FIXTURE_ACCOUNT, FIXTURE_DEPOSIT_AMOUNT);
        vm.prank(FIXTURE_ACCOUNT);
        target.deposit{value: FIXTURE_DEPOSIT_AMOUNT}();
    }

    function _threeNativeDepositsHash(address alice, address bob) private pure returns (bytes32) {
        bytes memory encoded = abi.encodePacked("ZKCLOB_DEPOSITS_V1", uint64(3));
        encoded =
            bytes.concat(encoded, abi.encodePacked(uint64(0), FIXTURE_ACCOUNT, bytes32(0), FIXTURE_DEPOSIT_AMOUNT));
        encoded = bytes.concat(encoded, abi.encodePacked(uint64(1), alice, bytes32(0), uint128(1 ether)));
        encoded = bytes.concat(encoded, abi.encodePacked(uint64(2), bob, bytes32(0), uint128(2 ether)));
        return sha256(encoded);
    }
}
