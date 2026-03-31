// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "./BaseTest.sol";
import {SpillTest} from "src/SpillTest.sol";

contract SpillTestTest is BaseTest {
    SpillTest solRef = new SpillTest();
    address sirImpl = makeAddr("sir-implementation");

    function setUp() public {
        bytes memory sirInitcode = sir(abi.encode("src/spill_test.sir"));
        (bool initSucc,) = deployCodeTo(sirImpl, sirInitcode);
        assertTrue(initSucc, "sir init failed");
    }

    function test_sum18() public {
        // Build calldata: 18 words, values 1..18
        bytes memory dataIn = new bytes(576);
        for (uint256 i = 0; i < 18; i++) {
            assembly {
                mstore(add(add(dataIn, 0x20), mul(i, 0x20)), add(i, 1))
            }
        }
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
        // Expected: 1+2+...+18 = 171
        assertEq(abi.decode(sirOut, (uint256)), 171, "wrong sum");
    }
}
