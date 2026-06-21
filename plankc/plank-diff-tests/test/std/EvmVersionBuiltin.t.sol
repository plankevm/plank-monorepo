// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest, EvmVersion} from "../BaseTest.sol";

contract FixedBytesTest is BaseTest {
    address plankImplCancun = makeAddr("plank-impl-cancun");
    address plankImplPrague = makeAddr("plank-impl-prague");
    address plankImplOsaka = makeAddr("plank-impl-osaka");

    function setUp() public {
        bytes memory plankCodeCancun = plank("src/std/version_test.plk", EvmVersion.Cancun);
        vm.etch(plankImplCancun, plankCodeCancun);

        bytes memory plankCodePrague = plank("src/std/version_test.plk", EvmVersion.Prague);
        vm.etch(plankImplPrague, plankCodePrague);

        bytes memory plankCodeOsaka = plank("src/std/version_test.plk", EvmVersion.Osaka);
        vm.etch(plankImplOsaka, plankCodeOsaka);

    }

    function test_evm_version() public {
        bool success;

        (success,) = plankImplCancun.call(abi.encode(EvmVersion.Cancun));
        assertTrue(success);

        (success,) = plankImplPrague.call(abi.encode(EvmVersion.Prague));
        assertTrue(success);

        (success,) = plankImplOsaka.call(abi.encode(EvmVersion.Osaka));
        assertTrue(success);
    }
}
