// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Test, Vm} from "forge-std/Test.sol";
import {BaseTest} from "../BaseTest.sol";
import {MinimalProxyFactory} from "src/examples/MinimalProxy.sol";

contract TableLookupTest is BaseTest {
    address plankImpl;

    uint256 constant TOTAL_HASHES = 32;

    bytes32[TOTAL_HASHES] zero_hashes;

    function setUp() public {
        bytes memory plankCode = plank("src/examples/table_lookup.plk");
        plankImpl = deployCode(plankCode);

        for (uint256 height = 0; height < TOTAL_HASHES - 1; height++) {
            zero_hashes[height + 1] = sha256(abi.encodePacked(zero_hashes[height], zero_hashes[height]));
        }
    }

    function test_fuzzing_lookup_valid(uint256 index) public view {
        index = bound(index, 0, TOTAL_HASHES - 1);

        (bool succ, bytes memory ret) = plankImpl.staticcall(abi.encode(index));
        assertTrue(succ);
        assertEq(ret, abi.encode(zero_hashes[index]));
    }

    function test_fuzzing_lookup_invalid(uint256 index) public view {
        index = bound(index, TOTAL_HASHES, type(uint256).max);

        (bool succ, bytes memory ret) = plankImpl.staticcall(abi.encode(index));
        assertFalse(succ);
        assertEq(ret, "");
    }
}
