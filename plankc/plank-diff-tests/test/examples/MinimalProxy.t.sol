// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Test, Vm} from "forge-std/Test.sol";
import {BaseTest} from "../BaseTest.sol";
import {MinimalProxyFactory} from "src/examples/MinimalProxy.sol";

/// Simple implementation contract for testing clones
contract Counter {
    uint256 public value;

    function increment() external {
        value += 1;
    }

    function setValue(uint256 v) external {
        value = v;
    }
}

contract MinimalProxyTest is BaseTest {
    MinimalProxyFactory solRef;
    address plankImpl;
    Counter impl;

    function setUp() public {
        solRef = new MinimalProxyFactory();
        impl = new Counter();

        bytes memory plankCode = plank("src/examples/minimal_proxy.plk");
        plankImpl = deployCode(plankCode);
    }

    // --- clone ---

    function test_clone() public {
        (bool succ, bytes memory out) = plankImpl.call(abi.encodeWithSignature("clone(address)", address(impl)));
        require(succ, "plank clone failed");
        address cloneAddr = abi.decode(out, (address));

        // Clone should delegate to implementation
        Counter(cloneAddr).increment();
        assertEq(Counter(cloneAddr).value(), 1);
    }

    function test_clone_emits_event() public {
        vm.recordLogs();
        (bool succ,) = plankImpl.call(abi.encodeWithSignature("clone(address)", address(impl)));
        require(succ, "plank clone failed");

        Vm.Log[] memory logs = vm.getRecordedLogs();
        assertEq(logs.length, 1, "should emit one event");
        assertEq(logs[0].topics[0], keccak256("CloneCreated(address)"), "wrong event topic");
    }

    // --- cloneDeterministic ---

    function test_cloneDeterministic() public {
        bytes32 salt = bytes32(uint256(42));
        (bool succ, bytes memory out) =
            plankImpl.call(abi.encodeWithSignature("cloneDeterministic(address,bytes32)", address(impl), salt));
        require(succ, "plank deterministic clone failed");
        address cloneAddr = abi.decode(out, (address));

        // Clone should delegate to implementation
        Counter(cloneAddr).setValue(99);
        assertEq(Counter(cloneAddr).value(), 99);
    }

    // --- verify clone bytecode matches solidity ---

    function test_clone_bytecode_matches() public {
        address solClone = solRef.clone(address(impl));

        (bool succ, bytes memory out) = plankImpl.call(abi.encodeWithSignature("clone(address)", address(impl)));
        require(succ, "plank clone failed");
        address plankClone = abi.decode(out, (address));

        // Both should have identical runtime bytecode (the ERC1167 proxy)
        assertEq(solClone.code, plankClone.code, "proxy bytecode mismatch");
    }

    function test_deterministic_bytecode_matches() public {
        bytes32 salt = bytes32(uint256(123));
        address solClone = solRef.cloneDeterministic(address(impl), salt);

        (bool succ, bytes memory out) =
            plankImpl.call(abi.encodeWithSignature("cloneDeterministic(address,bytes32)", address(impl), salt));
        require(succ, "plank deterministic clone failed");
        address plankClone = abi.decode(out, (address));

        assertEq(solClone.code, plankClone.code, "proxy bytecode mismatch");
    }
}
