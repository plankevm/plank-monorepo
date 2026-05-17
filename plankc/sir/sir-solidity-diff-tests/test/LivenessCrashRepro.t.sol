// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Vm} from "forge-std/Test.sol";
import {BaseTest} from "./BaseTest.sol";

contract LivenessCrashReproTest is BaseTest {
    address sirImpl = makeAddr("sir-implementation");

    function setUp() public {
        bytes memory sirInitcode = sir(abi.encode("src/liveness_repro.sir"));
        (bool initSucc,) = deployCodeTo(sirImpl, sirInitcode);
        assertTrue(initSucc, "sir init failed");
    }

    function test_exists() public {
        assertGt(sirImpl.code.length, 0, "no code");
        (bool success, bytes memory ret) = sirImpl.call("");
        assertTrue(success);
        assertEq(ret, "");
    }
}
