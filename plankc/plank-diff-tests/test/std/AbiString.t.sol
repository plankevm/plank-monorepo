// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";
import {AbiStringTest} from "../../src/std/AbiStringTest.sol";

contract AbiStringDiffTest is BaseTest {
    function test_fuzzing_abiString(uint256 a, string memory s) public {
        address ref = address(new AbiStringTest());
        address impl = plankDeploy("src/std/abi_string_test.plk");
        assertCallEq(ref, impl, abi.encode(a, s));
    }

    // Pinned fuzz counterexample from 2026-07-28: a 9-byte string, i.e. 23 bytes
    // of tail padding. Regression guard for the zero-padding fix in abi_helpers.
    function test_pinnedNineByteString() public {
        address ref = address(new AbiStringTest());
        address impl = plankDeploy("src/std/abi_string_test.plk");
        bytes memory cd =
            hex"0000000000000000000012337b70f0ee07530f501d5f083bcf27c8e25cfb5bf800000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000009f09f95b47e603a725c0000000000000000000000000000000000000000000000";
        assertCallEq(ref, impl, cd);
    }
}
