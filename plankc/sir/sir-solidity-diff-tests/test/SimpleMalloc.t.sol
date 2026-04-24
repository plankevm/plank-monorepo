// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "./BaseTest.sol";

/// @author philogy <https://github.com/philogy>
contract SimpleMallocTest is BaseTest {
    address sirImpl = makeAddr("sir-implementation");

    function setUp() public {
        bytes memory sirCode = sir(abi.encode("src/simple_malloc.sir"));
        deployCodeTo(sirImpl, sirCode);
    }

    function test_fuzzing_mallocBug(uint256 value1, uint256 value2) public {
        bytes memory dataIn = abi.encode(value1, value2);
        (bool success, bytes memory out) = sirImpl.call(dataIn);

        assertTrue(success);
        assertEq(out.length, 32);
        assertEq(abi.decode(out, (uint256)), value1);
    }
}
