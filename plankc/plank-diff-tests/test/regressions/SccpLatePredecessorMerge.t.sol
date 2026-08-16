// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "test/BaseTest.sol";

contract SccpLatePredecessorMergeTest is BaseTest {
    string constant SOURCE = "src/regressions/sccp_late_predecessor_merge.plk";

    address sirO0 = makeAddr("sccp-late-predecessor-merge-sir-o0");
    address sirO2 = makeAddr("sccp-late-predecessor-merge-sir-o2");

    function setUp() public {
        vm.etch(sirO0, plankBuild(SOURCE, baseBuildOptions().withBackend("sir").withOptimizations("O0")));
        vm.etch(sirO2, plankBuild(SOURCE, baseBuildOptions().withBackend("sir").withOptimizations("O2")));
    }

    function test_sirO2MatchesSirO0ForLatePredecessorMerge() public {
        bytes memory data = hex"0000000000000000000000000000000000000000000000000000000000000001";

        bytes memory expected = callAndReturn(sirO0, data);
        bytes memory actual = callAndReturn(sirO2, data);

        assertEq(abi.decode(expected, (uint256)), 1);
        assertEq(actual, expected);
    }

    function callAndReturn(address target, bytes memory data) internal returns (bytes memory out) {
        bool success;
        (success, out) = target.call(data);
        assertTrue(success);
        assertEq(out.length, 32);
    }
}
