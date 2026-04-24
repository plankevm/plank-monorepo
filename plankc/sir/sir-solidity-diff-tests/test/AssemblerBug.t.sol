// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "./BaseTest.sol";
import {AssemblerBug} from "src/AssemblerBug.sol";

/// @author philogy <https://github.com/philogy>
contract AssemblerBugTest is BaseTest {
    AssemblerBug solRef = new AssemblerBug();
    address sirImpl = makeAddr("sir-implementation");

    function setUp() public {
        bytes memory sirCode = sir(abi.encode("src/assembler_bug.sir"));
        deployCodeTo(sirImpl, sirCode);
    }

    function test_fuzzing_assemblerBug(uint256 x, uint256 y) public {
        bytes memory dataIn = abi.encode(x, y);
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }
}
