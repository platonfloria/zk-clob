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
    address private constant FIXTURE_ALICE_ACCOUNT = 0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf;
    address private constant FIXTURE_CAROL_ACCOUNT = 0x1efF47bc3a10a45D4B230B5d10E37751FE6AA718;
    address private constant FIXTURE_USDC = 0x0202020202020202020202020202020202020202;
    uint128 private constant FIXTURE_DEPOSIT_AMOUNT = 1 ether;
    uint128 private constant FIXTURE_WITHDRAWAL_AMOUNT = 100_000_000;
    uint128 private constant FIXTURE_FORCED_WITHDRAWAL_AMOUNT = 20_000_000;
    uint64 private constant FORCED_WITHDRAWAL_DELAY = 360;

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

    event WithdrawalExecuted(
        address indexed account, address indexed recipient, bytes32 indexed asset, uint128 amount, uint64 nonce
    );

    event ForcedWithdrawalRequested(
        uint64 indexed id, address indexed account, address indexed asset, uint128 amount, uint64 deadline
    );

    event ForcedWithdrawalExecuted(address indexed account, bytes32 indexed asset, uint128 amount, uint64 indexed id);

    event EscapeModeActivated(uint64 requestId, uint64 deadline);

    function setUp() public {
        publicValues = vm.readFileBinary(PUBLIC_VALUES_PATH);
        proof = vm.readFileBinary(PROOF_PATH);
        programVKey = vm.parseBytes32(vm.readFile(PROGRAM_VKEY_PATH));
        output = abi.decode(publicValues, (IZkClob.PublicOutput));

        vm.chainId(output.domain.chainId);

        mockVerifier = new MockSP1Verifier();
        exchange = _deploy(ISP1Verifier(address(mockVerifier)));
        _queueFixtureDeposit(exchange);
        _fundFixtureWithdrawal(exchange);
        _queueFixtureForcedWithdrawal(exchange);
    }

    function test_SettleUpdatesStateUsingFixture() public {
        vm.expectEmit(true, true, true, true);
        emit WithdrawalExecuted(
            FIXTURE_ALICE_ACCOUNT, FIXTURE_ALICE_ACCOUNT, bytes32(uint256(uint160(FIXTURE_USDC))), FIXTURE_WITHDRAWAL_AMOUNT, 1
        );
        vm.expectEmit(true, true, true, true);
        emit ForcedWithdrawalExecuted(
            FIXTURE_CAROL_ACCOUNT, bytes32(uint256(uint160(FIXTURE_USDC))), FIXTURE_FORCED_WITHDRAWAL_AMOUNT, 0
        );
        vm.expectEmit(true, true, true, true);
        emit BatchSettled(output.batchId, output.oldStateRoot, output.newStateRoot, output.batchHash, output.tradesHash);

        exchange.settle(publicValues, proof, _fixtureWithdrawals(), _fixtureForcedWithdrawals());

        assertEq(exchange.stateRoot(), output.newStateRoot);
        assertEq(exchange.nextBatchId(), output.batchId + 1);
        assertEq(exchange.nextUnprocessedDeposit(), output.newDepositCursor);
        assertEq(
            exchange.nextUnprocessedForcedWithdrawal(),
            output.oldForcedWithdrawalCursor + _fixtureForcedWithdrawals().length
        );
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

    function test_SettleExecutesCommittedNativeWithdrawal() public {
        address account = makeAddr("withdrawal-account");
        address recipient = makeAddr("withdrawal-recipient");
        uint128 amount = 0.25 ether;
        vm.deal(address(exchange), address(exchange).balance + amount);

        IZkClob.Withdrawal[] memory withdrawals = new IZkClob.Withdrawal[](1);
        withdrawals[0] = IZkClob.Withdrawal(account, recipient, bytes32(0), amount, 7);
        IZkClob.PublicOutput memory changed = output;
        changed.withdrawalsHash = _withdrawalsHash(withdrawals);

        vm.expectEmit(true, true, true, true);
        emit WithdrawalExecuted(account, recipient, bytes32(0), amount, 7);
        exchange.settle(abi.encode(changed), proof, withdrawals, _fixtureForcedWithdrawals());

        assertEq(recipient.balance, amount);
    }

    function test_SettleExecutesCommittedErc20Withdrawal() public {
        address account = makeAddr("token-withdrawal-account");
        address recipient = makeAddr("token-withdrawal-recipient");
        uint128 amount = 1_000_000;
        MockERC20 token = new MockERC20();
        token.mint(address(exchange), amount);

        IZkClob.Withdrawal[] memory withdrawals = new IZkClob.Withdrawal[](1);
        withdrawals[0] = IZkClob.Withdrawal(account, recipient, bytes32(uint256(uint160(address(token)))), amount, 3);
        IZkClob.PublicOutput memory changed = output;
        changed.withdrawalsHash = _withdrawalsHash(withdrawals);

        exchange.settle(abi.encode(changed), proof, withdrawals, _fixtureForcedWithdrawals());

        assertEq(token.balanceOf(recipient), amount);
        assertEq(token.balanceOf(address(exchange)), 0);
    }

    function test_WrongWithdrawalsHashRevertsBeforeTransfer() public {
        address recipient = makeAddr("uncommitted-recipient");
        IZkClob.Withdrawal[] memory withdrawals = new IZkClob.Withdrawal[](1);
        withdrawals[0] = IZkClob.Withdrawal(makeAddr("account"), recipient, bytes32(0), 1, 0);
        bytes32 actual = _withdrawalsHash(withdrawals);

        vm.expectRevert(
            abi.encodeWithSelector(IZkClob.WithdrawalsHashMismatch.selector, actual, output.withdrawalsHash)
        );
        exchange.settle(publicValues, proof, withdrawals, _fixtureForcedWithdrawals());

        assertEq(recipient.balance, 0);
    }

    function test_RealGroth16ProofFromTestdataVerifies() public {
        SP1Groth16Verifier verifier = new SP1Groth16Verifier();
        vm.etch(address(mockVerifier), address(verifier).code);

        exchange.settle(publicValues, proof, _fixtureWithdrawals(), _fixtureForcedWithdrawals());

        assertEq(exchange.stateRoot(), output.newStateRoot);
        assertEq(exchange.nextBatchId(), output.batchId + 1);
        assertEq(exchange.nextUnprocessedDeposit(), output.newDepositCursor);
        assertEq(
            exchange.nextUnprocessedForcedWithdrawal(),
            output.oldForcedWithdrawalCursor + _fixtureForcedWithdrawals().length
        );
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

        exchange.settle(abi.encode(changed), proof, _fixtureWithdrawals(), _fixtureForcedWithdrawals());

        assertEq(exchange.nextUnprocessedDeposit(), 3);
        assertEq(exchange.stateRoot(), changed.newStateRoot);
    }

    function test_WrongOldDepositCursorReverts() public {
        IZkClob.PublicOutput memory changed = output;
        changed.oldDepositCursor = 2;
        changed.newDepositCursor = 2;

        vm.expectRevert(abi.encodeWithSelector(IZkClob.WrongDepositCursor.selector, 0, 2));
        exchange.settle(abi.encode(changed), proof, _emptyWithdrawals(), _emptyForcedWithdrawals());
    }

    function test_DepositCursorCannotAdvancePastQueue() public {
        IZkClob.PublicOutput memory changed = output;
        changed.newDepositCursor = 2;

        vm.expectRevert(abi.encodeWithSelector(IZkClob.InvalidDepositCursorAdvance.selector, 0, 2, 1));
        exchange.settle(abi.encode(changed), proof, _emptyWithdrawals(), _emptyForcedWithdrawals());
    }

    function test_WrongConsumedDepositsHashReverts() public {
        IZkClob.PublicOutput memory changed = output;
        changed.consumedDepositsHash = bytes32(uint256(changed.consumedDepositsHash) + 1);
        bytes32 expected = sha256(
            abi.encodePacked(
                "ZKCLOB_DEPOSITS_V1", uint64(1), uint64(0), FIXTURE_ALICE_ACCOUNT, bytes32(0), FIXTURE_DEPOSIT_AMOUNT
            )
        );

        vm.expectRevert(
            abi.encodeWithSelector(
                IZkClob.ConsumedDepositsHashMismatch.selector, expected, changed.consumedDepositsHash
            )
        );
        exchange.settle(abi.encode(changed), proof, _emptyWithdrawals(), _emptyForcedWithdrawals());
    }

    function test_WrongOldForcedWithdrawalCursorReverts() public {
        IZkClob.PublicOutput memory changed = output;
        changed.oldForcedWithdrawalCursor = 5;

        vm.expectRevert(abi.encodeWithSelector(IZkClob.WrongForcedWithdrawalCursor.selector, 0, 5));
        exchange.settle(abi.encode(changed), proof, _emptyWithdrawals(), _emptyForcedWithdrawals());
    }

    function test_ForcedWithdrawalCursorCannotAdvancePastQueue() public {
        IZkClob.ForcedWithdrawal[] memory forcedWithdrawals = new IZkClob.ForcedWithdrawal[](2);
        forcedWithdrawals[0] =
            IZkClob.ForcedWithdrawal(0, FIXTURE_CAROL_ACCOUNT, bytes32(uint256(uint160(FIXTURE_USDC))), 0);
        forcedWithdrawals[1] =
            IZkClob.ForcedWithdrawal(1, FIXTURE_CAROL_ACCOUNT, bytes32(uint256(uint160(FIXTURE_USDC))), 0);

        vm.expectRevert(abi.encodeWithSelector(IZkClob.InvalidForcedWithdrawalCursorAdvance.selector, 0, 2, 1));
        exchange.settle(publicValues, proof, _emptyWithdrawals(), forcedWithdrawals);
    }

    function test_WrongConsumedForcedWithdrawalsHashReverts() public {
        IZkClob.PublicOutput memory changed = output;
        changed.consumedForcedWithdrawalsHash = bytes32(uint256(changed.consumedForcedWithdrawalsHash) + 1);
        bytes32 expected = sha256(
            abi.encodePacked(
                "ZKCLOB_FORCED_WITHDRAWALS_V1",
                uint64(1),
                uint64(0),
                FIXTURE_CAROL_ACCOUNT,
                bytes32(uint256(uint160(FIXTURE_USDC))),
                FIXTURE_FORCED_WITHDRAWAL_AMOUNT
            )
        );

        vm.expectRevert(
            abi.encodeWithSelector(
                IZkClob.ConsumedForcedWithdrawalsHashMismatch.selector,
                expected,
                changed.consumedForcedWithdrawalsHash
            )
        );
        exchange.settle(abi.encode(changed), proof, _emptyWithdrawals(), _fixtureForcedWithdrawals());
    }

    function test_WrongForcedWithdrawalsHashRevertsBeforeTransfer() public {
        IZkClob.ForcedWithdrawal[] memory forcedWithdrawals = new IZkClob.ForcedWithdrawal[](1);
        forcedWithdrawals[0] =
            IZkClob.ForcedWithdrawal(0, FIXTURE_CAROL_ACCOUNT, bytes32(uint256(uint160(FIXTURE_USDC))), 1);
        bytes32 actual = _forcedWithdrawalsHash(forcedWithdrawals);

        vm.expectRevert(
            abi.encodeWithSelector(IZkClob.ForcedWithdrawalsHashMismatch.selector, actual, output.forcedWithdrawalsHash)
        );
        exchange.settle(publicValues, proof, _fixtureWithdrawals(), forcedWithdrawals);

        assertEq(MockERC20(FIXTURE_USDC).balanceOf(FIXTURE_CAROL_ACCOUNT), 0);
    }

    function test_SettleExecutesCommittedForcedWithdrawal() public {
        uint128 amount = 3_000_000;
        IZkClob.ForcedWithdrawal[] memory forcedWithdrawals = new IZkClob.ForcedWithdrawal[](1);
        forcedWithdrawals[0] =
            IZkClob.ForcedWithdrawal(0, FIXTURE_CAROL_ACCOUNT, bytes32(uint256(uint160(FIXTURE_USDC))), amount);
        IZkClob.PublicOutput memory changed = output;
        changed.forcedWithdrawalsHash = _forcedWithdrawalsHash(forcedWithdrawals);

        vm.expectEmit(true, true, true, true);
        emit ForcedWithdrawalExecuted(FIXTURE_CAROL_ACCOUNT, bytes32(uint256(uint160(FIXTURE_USDC))), amount, 0);
        exchange.settle(abi.encode(changed), proof, _fixtureWithdrawals(), forcedWithdrawals);

        assertEq(MockERC20(FIXTURE_USDC).balanceOf(FIXTURE_CAROL_ACCOUNT), amount);
    }

    function test_RequestForcedWithdrawalQueuesRequestAndEmitsEvent() public {
        address user = makeAddr("forced-withdrawer");
        uint128 amount = 5_000_000;
        uint64 expectedDeadline = uint64(block.timestamp) + FORCED_WITHDRAWAL_DELAY;

        vm.expectEmit(true, true, true, true);
        emit ForcedWithdrawalRequested(1, user, FIXTURE_USDC, amount, expectedDeadline);
        vm.prank(user);
        uint64 requestId = exchange.requestForcedWithdrawal(FIXTURE_USDC, amount);

        assertEq(requestId, 1);
        assertEq(exchange.nextForcedWithdrawalId(), 2);
        (address account, address asset, uint128 queuedAmount, uint64 deadline) =
            exchange.forcedWithdrawalRequests(requestId);
        assertEq(account, user);
        assertEq(asset, FIXTURE_USDC);
        assertEq(queuedAmount, amount);
        assertEq(deadline, expectedDeadline);
    }

    function test_ZeroForcedWithdrawalAmountReverts() public {
        vm.expectRevert(IZkClob.ZeroForcedWithdrawalAmount.selector);
        exchange.requestForcedWithdrawal(FIXTURE_USDC, 0);

        assertEq(exchange.nextForcedWithdrawalId(), 1);
    }

    function test_ActivateEscapeModeRevertsWithEmptyQueue() public {
        exchange.settle(publicValues, proof, _fixtureWithdrawals(), _fixtureForcedWithdrawals());

        vm.expectRevert(IZkClob.NoPendingForcedWithdrawal.selector);
        exchange.activateEscapeMode();
    }

    function test_ActivateEscapeModeRevertsBeforeDeadline() public {
        uint64 deadline = uint64(block.timestamp) + FORCED_WITHDRAWAL_DELAY;

        vm.expectRevert(abi.encodeWithSelector(IZkClob.ForcedWithdrawalDeadlineNotElapsed.selector, deadline));
        exchange.activateEscapeMode();
    }

    function test_ActivateEscapeModeSucceedsAfterDeadlineAndBlocksSettle() public {
        uint64 deadline = uint64(block.timestamp) + FORCED_WITHDRAWAL_DELAY;
        vm.warp(deadline);

        vm.expectEmit(true, true, true, true);
        emit EscapeModeActivated(0, deadline);
        exchange.activateEscapeMode();

        assertTrue(exchange.escapeMode());

        vm.expectRevert(IZkClob.EscapeModeActive.selector);
        exchange.settle(publicValues, proof, _fixtureWithdrawals(), _fixtureForcedWithdrawals());
    }

    function test_ReplayReverts() public {
        exchange.settle(publicValues, proof, _fixtureWithdrawals(), _fixtureForcedWithdrawals());

        vm.expectRevert(abi.encodeWithSelector(IZkClob.WrongBatchId.selector, output.batchId + 1, output.batchId));
        exchange.settle(publicValues, proof, _fixtureWithdrawals(), _fixtureForcedWithdrawals());
    }

    function test_VerifierRejectionLeavesStateUnchanged() public {
        mockVerifier.setRejectProof(true);

        vm.expectRevert(MockSP1Verifier.ProofRejected.selector);
        exchange.settle(publicValues, proof, _fixtureWithdrawals(), _fixtureForcedWithdrawals());

        assertEq(exchange.stateRoot(), output.oldStateRoot);
        assertEq(exchange.nextBatchId(), output.batchId);
    }

    function test_InvalidPublicValuesLengthReverts() public {
        bytes memory truncated = new bytes(publicValues.length - 1);

        vm.expectRevert(abi.encodeWithSelector(IZkClob.InvalidPublicValuesLength.selector, truncated.length));
        exchange.settle(truncated, proof, _emptyWithdrawals(), _emptyForcedWithdrawals());
    }

    function test_MalformedPublicValuesRevert() public {
        bytes memory malformed = publicValues;
        // A uint32 occupies the low four bytes of its ABI word. Setting a high
        // padding byte makes the otherwise correctly sized encoding invalid.
        malformed[0] = 0x01;

        vm.expectRevert();
        exchange.settle(malformed, proof, _emptyWithdrawals(), _emptyForcedWithdrawals());
    }

    function test_ProofForDifferentProgramVKeyReverts() public {
        bytes32 oldProgramVKey = vm.parseBytes32(vm.readFile(OLD_PROGRAM_VKEY_PATH));
        SP1Groth16Verifier verifier = new SP1Groth16Verifier();

        vm.expectRevert();
        verifier.verifyProof(oldProgramVKey, publicValues, proof);
    }

    function test_WrongProtocolVersionReverts() public {
        IZkClob.PublicOutput memory changed = output;
        changed.domain.protocolVersion++;

        vm.expectRevert(
            abi.encodeWithSelector(
                IZkClob.WrongProtocolVersion.selector, output.domain.protocolVersion, changed.domain.protocolVersion
            )
        );
        exchange.settle(abi.encode(changed), proof, _emptyWithdrawals(), _emptyForcedWithdrawals());
    }

    function test_WrongChainReverts() public {
        uint64 actualChainId = output.domain.chainId + 1;
        vm.chainId(actualChainId);

        vm.expectRevert(
            abi.encodeWithSelector(IZkClob.WrongChain.selector, uint256(actualChainId), output.domain.chainId)
        );
        exchange.settle(publicValues, proof, _emptyWithdrawals(), _emptyForcedWithdrawals());
    }

    function test_WrongExchangeReverts() public {
        IZkClob.PublicOutput memory changed = output;
        changed.domain.exchangeId = address(uint160(output.domain.exchangeId) + 1);

        vm.expectRevert(
            abi.encodeWithSelector(IZkClob.WrongExchange.selector, output.domain.exchangeId, changed.domain.exchangeId)
        );
        exchange.settle(abi.encode(changed), proof, _emptyWithdrawals(), _emptyForcedWithdrawals());
    }

    function test_WrongConfigReverts() public {
        IZkClob.PublicOutput memory changed = output;
        changed.configHash = bytes32(uint256(output.configHash) + 1);

        vm.expectRevert(abi.encodeWithSelector(IZkClob.WrongConfig.selector, output.configHash, changed.configHash));
        exchange.settle(abi.encode(changed), proof, _emptyWithdrawals(), _emptyForcedWithdrawals());
    }

    function test_WrongBatchIdReverts() public {
        IZkClob.PublicOutput memory changed = output;
        changed.batchId++;

        vm.expectRevert(abi.encodeWithSelector(IZkClob.WrongBatchId.selector, output.batchId, changed.batchId));
        exchange.settle(abi.encode(changed), proof, _emptyWithdrawals(), _emptyForcedWithdrawals());
    }

    function test_StaleStateRootReverts() public {
        IZkClob.PublicOutput memory changed = output;
        changed.oldStateRoot = bytes32(uint256(output.oldStateRoot) + 1);

        vm.expectRevert(
            abi.encodeWithSelector(IZkClob.StaleStateRoot.selector, output.oldStateRoot, changed.oldStateRoot)
        );
        exchange.settle(abi.encode(changed), proof, _emptyWithdrawals(), _emptyForcedWithdrawals());
    }

    function test_ZeroVerifierReverts() public {
        vm.expectRevert(IZkClob.ZeroVerifier.selector);
        new ZkClob(
            ISP1Verifier(address(0)),
            programVKey,
            output.configHash,
            output.domain.protocolVersion,
            output.oldStateRoot,
            output.batchId,
            FORCED_WITHDRAWAL_DELAY
        );
    }

    function _deploy(ISP1Verifier verifier) private returns (ZkClob) {
        return new ZkClob(
            verifier,
            programVKey,
            output.configHash,
            output.domain.protocolVersion,
            output.oldStateRoot,
            output.batchId,
            FORCED_WITHDRAWAL_DELAY
        );
    }

    function _queueFixtureDeposit(ZkClob target) private {
        vm.deal(FIXTURE_ALICE_ACCOUNT, FIXTURE_DEPOSIT_AMOUNT);
        vm.prank(FIXTURE_ALICE_ACCOUNT);
        target.deposit{value: FIXTURE_DEPOSIT_AMOUNT}();
    }

    function _fundFixtureWithdrawal(ZkClob target) private {
        if (FIXTURE_USDC.code.length == 0) {
            MockERC20 implementation = new MockERC20();
            vm.etch(FIXTURE_USDC, address(implementation).code);
        }
        MockERC20(FIXTURE_USDC).mint(address(target), FIXTURE_WITHDRAWAL_AMOUNT + FIXTURE_FORCED_WITHDRAWAL_AMOUNT);
    }

    function _queueFixtureForcedWithdrawal(ZkClob target) private {
        vm.prank(FIXTURE_CAROL_ACCOUNT);
        target.requestForcedWithdrawal(FIXTURE_USDC, FIXTURE_FORCED_WITHDRAWAL_AMOUNT);
    }

    function _fixtureWithdrawals() private pure returns (IZkClob.Withdrawal[] memory withdrawals) {
        withdrawals = new IZkClob.Withdrawal[](1);
        withdrawals[0] = IZkClob.Withdrawal(
            FIXTURE_ALICE_ACCOUNT, FIXTURE_ALICE_ACCOUNT, bytes32(uint256(uint160(FIXTURE_USDC))), FIXTURE_WITHDRAWAL_AMOUNT, 1
        );
    }

    function _emptyWithdrawals() private pure returns (IZkClob.Withdrawal[] memory) {
        return new IZkClob.Withdrawal[](0);
    }

    function _fixtureForcedWithdrawals() private pure returns (IZkClob.ForcedWithdrawal[] memory forcedWithdrawals) {
        forcedWithdrawals = new IZkClob.ForcedWithdrawal[](1);
        forcedWithdrawals[0] = IZkClob.ForcedWithdrawal(
            0, FIXTURE_CAROL_ACCOUNT, bytes32(uint256(uint160(FIXTURE_USDC))), FIXTURE_FORCED_WITHDRAWAL_AMOUNT
        );
    }

    function _emptyForcedWithdrawals() private pure returns (IZkClob.ForcedWithdrawal[] memory) {
        return new IZkClob.ForcedWithdrawal[](0);
    }

    function _withdrawalsHash(IZkClob.Withdrawal[] memory withdrawals) private pure returns (bytes32) {
        bytes memory encoded = abi.encodePacked("ZKCLOB_WITHDRAWALS_V1", uint64(withdrawals.length));
        for (uint256 index; index < withdrawals.length; index++) {
            IZkClob.Withdrawal memory withdrawal = withdrawals[index];
            encoded = bytes.concat(
                encoded,
                abi.encodePacked(
                    withdrawal.account, withdrawal.recipient, withdrawal.asset, withdrawal.amount, withdrawal.nonce
                )
            );
        }
        return sha256(encoded);
    }

    function _forcedWithdrawalsHash(IZkClob.ForcedWithdrawal[] memory forcedWithdrawals)
        private
        pure
        returns (bytes32)
    {
        bytes memory encoded = abi.encodePacked("ZKCLOB_FORCED_WITHDRAWALS_V1", uint64(forcedWithdrawals.length));
        for (uint256 index; index < forcedWithdrawals.length; index++) {
            IZkClob.ForcedWithdrawal memory forcedWithdrawal = forcedWithdrawals[index];
            encoded = bytes.concat(
                encoded,
                abi.encodePacked(
                    forcedWithdrawal.id, forcedWithdrawal.account, forcedWithdrawal.asset, forcedWithdrawal.amount
                )
            );
        }
        return sha256(encoded);
    }

    function _threeNativeDepositsHash(address alice, address bob) private pure returns (bytes32) {
        bytes memory encoded = abi.encodePacked("ZKCLOB_DEPOSITS_V1", uint64(3));
        encoded =
            bytes.concat(encoded, abi.encodePacked(uint64(0), FIXTURE_ALICE_ACCOUNT, bytes32(0), FIXTURE_DEPOSIT_AMOUNT));
        encoded = bytes.concat(encoded, abi.encodePacked(uint64(1), alice, bytes32(0), uint128(1 ether)));
        encoded = bytes.concat(encoded, abi.encodePacked(uint64(2), bob, bytes32(0), uint128(2 ether)));
        return sha256(encoded);
    }
}
