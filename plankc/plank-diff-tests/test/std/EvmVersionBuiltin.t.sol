// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";

contract EvmVersionTest is BaseTest {
    address plankImplCancun = makeAddr("plank-impl-cancun");
    address plankImplPrague = makeAddr("plank-impl-prague");
    address plankImplOsaka = makeAddr("plank-impl-osaka");

    function setUp() public {
        string memory sourceFile = "src/std/version_test.plk";
        vm.etch(plankImplCancun, compileForVersion(sourceFile, "cancun"));
        vm.etch(plankImplPrague, compileForVersion(sourceFile, "prague"));
        vm.etch(plankImplOsaka, plank(sourceFile));
    }

    function compileForVersion(string memory sourceFile, string memory version) internal returns (bytes memory) {
        string[] memory extraArgs = new string[](2);
        extraArgs[0] = "--evm-version";
        extraArgs[1] = version;
        return plank(sourceFile, extraArgs);
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
