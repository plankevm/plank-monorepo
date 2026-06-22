// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "./BaseTest.sol";

contract CriticalEdgeTest is BaseTest {
    address impl = makeAddr("sir-critical-edge");

    function setUp() public {
        vm.etch(impl, sir(abi.encode("src/critical_edge.sir", "--init-only")));
    }

    function test_fuzzing_revert_noData(uint256 value) public {
        vm.deal(address(this), value);
        (bool succ, bytes memory data) = impl.call{value: value}("");
        assertFalse(succ);
        assertEq(data, "");
    }

    function test_fuzzing_revert_noData(bytes calldata input) public {
        (bool succ, bytes memory data) = impl.call{value: 0}(input);
        assertFalse(succ);
        assertEq(data, "");
    }

    function test_fuzzing_call(uint256 value, bytes calldata b) public {
        vm.assume(b.length > 0);
        value = bound(value, 1, type(uint256).max);

        vm.deal(address(this), value);
        (bool succ, bytes memory data) = impl.call{value: value}(b);

        assertTrue(succ);
        assertEq(data, "");
        assertEq(vm.load(impl, bytes32(0)), bytes32(value));
    }
}
