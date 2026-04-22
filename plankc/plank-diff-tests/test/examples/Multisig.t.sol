// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Test, Vm} from "forge-std/Test.sol";
import {BaseTest} from "../BaseTest.sol";
import {Multisig} from "src/examples/Multisig.sol";

contract Receiver {
    uint256 public value;

    receive() external payable {
        value = msg.value;
    }
}

contract MultisigTest is BaseTest {
    Multisig solRef;
    address plankImpl;

    address owner0;
    address owner1;
    address owner2;
    address nonOwner;

    function setUp() public {
        owner0 = makeAddr("owner0");
        owner1 = makeAddr("owner1");
        owner2 = makeAddr("owner2");
        nonOwner = makeAddr("nonOwner");

        address[3] memory owners = [owner0, owner1, owner2];
        solRef = new Multisig(owners);

        bytes memory plankCode = plank("src/examples/multisig.plk");
        plankImpl = deployCode(plankCode, abi.encode(owners));
    }

    // --- helpers ---

    function assertCallEq(bytes memory data) internal {
        assertCallEq(address(solRef), plankImpl, data);
    }

    function assertCallEqFrom(bytes memory data, address sender) internal {
        assertCallEqFrom(address(solRef), plankImpl, data, sender);
    }

    // --- submit ---

    function test_submit() public {
        assertCallEqFrom(
            abi.encodeWithSignature("submitTransaction(address,uint256,bytes)", address(0xdead), 0, hex"1234"),
            owner0
        );
    }

    function test_submit_reverts_nonOwner() public {
        assertCallEqFrom(
            abi.encodeWithSignature("submitTransaction(address,uint256,bytes)", address(0xdead), 0, hex""),
            nonOwner
        );
    }

    // --- confirm ---

    function test_confirm() public {
        assertCallEqFrom(
            abi.encodeWithSignature("submitTransaction(address,uint256,bytes)", address(0xdead), 0, hex""),
            owner0
        );
        assertCallEqFrom(
            abi.encodeWithSignature("confirmTransaction(uint256)", 0),
            owner0
        );
    }

    function test_confirm_reverts_already_confirmed() public {
        assertCallEqFrom(
            abi.encodeWithSignature("submitTransaction(address,uint256,bytes)", address(0xdead), 0, hex""),
            owner0
        );
        assertCallEqFrom(
            abi.encodeWithSignature("confirmTransaction(uint256)", 0),
            owner0
        );
        assertCallEqFrom(
            abi.encodeWithSignature("confirmTransaction(uint256)", 0),
            owner0
        );
    }

    function test_confirm_reverts_invalid_txId() public {
        assertCallEqFrom(
            abi.encodeWithSignature("confirmTransaction(uint256)", 99),
            owner0
        );
    }

    function test_confirm_reverts_nonOwner() public {
        assertCallEqFrom(
            abi.encodeWithSignature("submitTransaction(address,uint256,bytes)", address(0xdead), 0, hex""),
            owner0
        );
        assertCallEqFrom(
            abi.encodeWithSignature("confirmTransaction(uint256)", 0),
            nonOwner
        );
    }

    // --- revoke ---

    function test_revoke() public {
        assertCallEqFrom(
            abi.encodeWithSignature("submitTransaction(address,uint256,bytes)", address(0xdead), 0, hex""),
            owner0
        );
        assertCallEqFrom(
            abi.encodeWithSignature("confirmTransaction(uint256)", 0),
            owner0
        );
        assertCallEqFrom(
            abi.encodeWithSignature("revokeConfirmation(uint256)", 0),
            owner0
        );
    }

    function test_revoke_reverts_not_confirmed() public {
        assertCallEqFrom(
            abi.encodeWithSignature("submitTransaction(address,uint256,bytes)", address(0xdead), 0, hex""),
            owner0
        );
        assertCallEqFrom(
            abi.encodeWithSignature("revokeConfirmation(uint256)", 0),
            owner0
        );
    }

    // --- getConfirmationCount ---

    function test_getConfirmationCount_zero() public {
        assertCallEqFrom(
            abi.encodeWithSignature("submitTransaction(address,uint256,bytes)", address(0xdead), 0, hex""),
            owner0
        );
        assertCallEq(abi.encodeWithSignature("getConfirmationCount(uint256)", 0));
    }

    function test_getConfirmationCount_two() public {
        assertCallEqFrom(
            abi.encodeWithSignature("submitTransaction(address,uint256,bytes)", address(0xdead), 0, hex""),
            owner0
        );
        assertCallEqFrom(
            abi.encodeWithSignature("confirmTransaction(uint256)", 0),
            owner0
        );
        assertCallEqFrom(
            abi.encodeWithSignature("confirmTransaction(uint256)", 0),
            owner1
        );
        assertCallEq(abi.encodeWithSignature("getConfirmationCount(uint256)", 0));
    }

    // --- getTransaction ---

    function test_getTransaction() public {
        assertCallEqFrom(
            abi.encodeWithSignature("submitTransaction(address,uint256,bytes)", address(0xdead), 123, hex"aabbccdd"),
            owner0
        );
        assertCallEq(abi.encodeWithSignature("getTransaction(uint256)", 0));
    }

    // --- execute ---

    function test_execute() public {
        Receiver recv = new Receiver();
        vm.deal(address(solRef), 1 ether);
        vm.deal(plankImpl, 1 ether);

        assertCallEqFrom(
            abi.encodeWithSignature("submitTransaction(address,uint256,bytes)", address(recv), 100, hex""),
            owner0
        );
        assertCallEqFrom(
            abi.encodeWithSignature("confirmTransaction(uint256)", 0),
            owner0
        );
        assertCallEqFrom(
            abi.encodeWithSignature("confirmTransaction(uint256)", 0),
            owner1
        );
        assertCallEqFrom(
            abi.encodeWithSignature("executeTransaction(uint256)", 0),
            owner0
        );
    }

    function test_execute_reverts_below_threshold() public {
        assertCallEqFrom(
            abi.encodeWithSignature("submitTransaction(address,uint256,bytes)", address(0xdead), 0, hex""),
            owner0
        );
        assertCallEqFrom(
            abi.encodeWithSignature("confirmTransaction(uint256)", 0),
            owner0
        );
        assertCallEqFrom(
            abi.encodeWithSignature("executeTransaction(uint256)", 0),
            owner0
        );
    }

    function test_execute_reverts_already_executed() public {
        Receiver recv = new Receiver();
        vm.deal(address(solRef), 1 ether);
        vm.deal(plankImpl, 1 ether);

        assertCallEqFrom(
            abi.encodeWithSignature("submitTransaction(address,uint256,bytes)", address(recv), 0, hex""),
            owner0
        );
        assertCallEqFrom(
            abi.encodeWithSignature("confirmTransaction(uint256)", 0),
            owner0
        );
        assertCallEqFrom(
            abi.encodeWithSignature("confirmTransaction(uint256)", 0),
            owner1
        );
        assertCallEqFrom(
            abi.encodeWithSignature("executeTransaction(uint256)", 0),
            owner0
        );
        assertCallEqFrom(
            abi.encodeWithSignature("executeTransaction(uint256)", 0),
            owner0
        );
    }

    function test_execute_reverts_nonOwner() public {
        assertCallEqFrom(
            abi.encodeWithSignature("submitTransaction(address,uint256,bytes)", address(0xdead), 0, hex""),
            owner0
        );
        assertCallEqFrom(
            abi.encodeWithSignature("confirmTransaction(uint256)", 0),
            owner0
        );
        assertCallEqFrom(
            abi.encodeWithSignature("confirmTransaction(uint256)", 0),
            owner1
        );
        assertCallEqFrom(
            abi.encodeWithSignature("executeTransaction(uint256)", 0),
            nonOwner
        );
    }
}
