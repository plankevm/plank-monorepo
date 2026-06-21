// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";

contract EvmVersionTest is BaseTest {
    address plankImplCancun = makeAddr("plank-impl-cancun");
    address plankImplPrague = makeAddr("plank-impl-prague");
    address plankImplOsaka = makeAddr("plank-impl-osaka");

    function setUp() public {
        string memory sourceFile = "src/std/version_test.plk";
        vm.etch(plankImplCancun, plank(sourceFile, "cancun"));
        vm.etch(plankImplPrague, plank(sourceFile, "prague"));
        vm.etch(plankImplOsaka, plank(sourceFile));
    }

    function test_evm_version() public {
        bool success;

        (success,) = plankImplCancun.call(abi.encode(0));
        assertTrue(success);

        (success,) = plankImplPrague.call(abi.encode(1));
        assertTrue(success);

        (success,) = plankImplOsaka.call(abi.encode(2));
        assertTrue(success);
    }
}
