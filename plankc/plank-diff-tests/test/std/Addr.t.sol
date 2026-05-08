// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";

contract AddrTest is BaseTest {
    address plankImpl = makeAddr("plank-impl");

    function setUp() public {
        bytes memory addrTestCode = plank("src/std/addr_test.plk");
        vm.etch(plankImpl, addrTestCode);
    }

    function test_addr_conversion_fails() public {
        (bool success,) = plankImpl.call(abi.encode(0));
        assertFalse(success);
    }

    function test_raw_create() public {
        (bool success,) = plankImpl.call(abi.encode(1));
        assertTrue(success);
    }
}
