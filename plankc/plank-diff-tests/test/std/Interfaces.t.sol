// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";

contract InterfacesTest is BaseTest {
    function test_interfaces() public {
        address instance = makeAddr("interfaces");
        vm.etch(instance, plank("src/std/interfaces_test.plk"));
        (bool success,) = instance.call("");
        assertTrue(success);
    }
}
