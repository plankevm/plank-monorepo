// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";

contract AddrTest is BaseTest {

    address plankImpl = makeAddr("plank-addr");

    function setUp() public {
        bytes memory addrTestCode = plank("src/std/addr_test.plk");
        vm.etch(plankImpl, addrTestCode);
    }

    function test_addr_conversion_fails() public {
        (bool success, bytes memory out) = plankImpl.call("");
        assertEq(success, false);
    }
}
