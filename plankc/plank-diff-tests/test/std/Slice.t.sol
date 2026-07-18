// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";

contract SliceTest is BaseTest {
    function test_sliceApis() public {
        address impl = plankDeploy("src/std/slice_test.plk");
        (bool success,) = impl.call(abi.encode(uint256(9), uint256(10), uint256(11), uint256(12)));
        assertTrue(success);
    }

    function test_atRevertsOutOfBounds() public {
        address impl = plankDeploy("src/std/slice_test.plk");
        (bool success,) = impl.call("");
        assertFalse(success);
    }

    function test_setRevertsOutOfBounds() public {
        address impl = plankDeploy("src/std/slice_test.plk");
        (bool success,) = impl.call(abi.encode(uint256(0)));
        assertFalse(success);
    }
}
