// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";

contract NewStorageTest is BaseTest {
    function test_storeAndLoadPackedNestedValue() public {
        plankDeploy("src/std/new_storage_test.plk");
    }

    function test_layoutV1() public {
        plankDeploy("src/std/new_storage_layout_test.plk");
    }
}
