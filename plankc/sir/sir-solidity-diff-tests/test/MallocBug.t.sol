// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "./BaseTest.sol";
import {MallocBug} from "src/MallocBug.sol";

/// @author philogy <https://github.com/philogy>
contract MallocBugTest is BaseTest {
    MallocBug solRef = new MallocBug();
    address sirImpl = makeAddr("sir-implementation");

    function setUp() public {
        bytes memory sirCode = sir(abi.encode("src/malloc_bug.sir"));
        deployCodeTo(sirImpl, sirCode);
    }

    function test_fuzzing_mallocBug(uint256 value1, uint256 value2) public {
        bytes memory dataIn = abi.encode(value1, value2);
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }
}
