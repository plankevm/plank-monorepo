// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";

contract FixedBytesTest is BaseTest {
    address plankImpl = makeAddr("plank-impl");

    function setUp() public {
        bytes memory fixedBytesTestCode = plank("src/std/fixedbytes_test.plk");
        vm.etch(plankImpl, fixedBytesTestCode);
    }

    function test_fixedbytes() public {
        (bool success,) = plankImpl.call("");
        assertTrue(success);
    }
}
