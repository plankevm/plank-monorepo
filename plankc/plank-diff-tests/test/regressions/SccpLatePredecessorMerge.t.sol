// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "test/BaseTest.sol";

contract SccpLatePredecessorMergeTest is BaseTest {
    string constant SOURCE = "src/regressions/sccp_late_predecessor_merge.plk";

    address sirDebug = makeAddr("sccp-late-predecessor-merge-sir-debug");
    address sirReleaseCsud = makeAddr("sccp-late-predecessor-merge-sir-release-csud");

    function setUp() public {
        vm.etch(sirDebug, plankBuild(SOURCE, baseBuildOptions().withBackend("sir-debug").disableOptimizations()));
        vm.etch(
            sirReleaseCsud, plankBuild(SOURCE, baseBuildOptions().withBackend("sir-release").withOptimizations("csud"))
        );
    }

    function test_sirReleaseCsudMatchesSirDebugForLatePredecessorMerge() public {
        bytes memory data = hex"0000000000000000000000000000000000000000000000000000000000000001";

        bytes memory expected = callAndReturn(sirDebug, data);
        bytes memory actual = callAndReturn(sirReleaseCsud, data);

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
