// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "./BaseTest.sol";

contract StackSpillTest is BaseTest {
    address sirImpl = makeAddr("sir-implementation");

    function setUp() public {
        bytes memory sirCode = sir(abi.encode("src/stack_spill.sir"));
        deployCodeTo(sirImpl, sirCode);
    }

    function test_fuzzing_stackSpill(uint256[24] memory values) public {
        (bool success, bytes memory out) = sirImpl.call(abi.encode(values));

        unchecked {
            uint256 sum;
            uint256 mix;
            for (uint256 i = 0; i < values.length; i++) {
                sum += values[i];
                mix ^= values[i];
            }

            assertTrue(success);
            assertEq(out, abi.encode(((sum * mix + values[17]) >> values[3]) ^ sum));
        }
    }
}
