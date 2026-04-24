// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "./BaseTest.sol";

/// @author philogy <https://github.com/philogy>
contract AssemblerStressTestTest is BaseTest {
    address sirImpl = makeAddr("sir-implementation");

    function setUp() public {
        bytes memory sirCode = sir(abi.encode("src/assembler_stress_test.sir"));
        deployCodeTo(sirImpl, sirCode);
    }

    function test_fuzzing_assemblerBug(bytes memory dataIn) public {
        (bool success, bytes memory out) = sirImpl.call(dataIn);

        assertTrue(success);
        assertEq(out, "");
    }
}
