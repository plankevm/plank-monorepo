// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";

contract MemTest is BaseTest {
    function test_roundTrip() public {
        plankDeploy("src/std/mem_test.plk");
    }
}
