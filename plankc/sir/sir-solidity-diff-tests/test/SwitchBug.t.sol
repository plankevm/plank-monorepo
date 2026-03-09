// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "./BaseTest.sol";
import {console} from "forge-std/console.sol";

/// @author philogy <https://github.com/philogy>
contract SwitchBugTest is BaseTest {
    address sirImpl = makeAddr("sir-implementation");
    bytes initcode = sir(abi.encode("src/switch_bug.sir", "--init-only"));

    function setUp() public {
        vm.etch(sirImpl, initcode);
    }

    function test_fuzzing_switchBug(uint256 x, uint256 y) public {
        console.log("x   : %x", x);
        console.log("y   : %x", y);
        uint256 diff = x > y ? x - y : y - x;
        console.log("diff: %x", diff);

        bytes memory dataIn = abi.encode(x, y);
        console.logBytes(dataIn);
        (bool success, bytes memory res) = sirImpl.call(dataIn);

        console.logBytes(initcode);
        console.logBytes(res);

        assertTrue(success);
        assertEq(res, abi.encode(diff));
    }
}

