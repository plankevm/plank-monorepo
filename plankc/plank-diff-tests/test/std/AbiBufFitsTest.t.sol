// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";
import {AbiBufFitsTest} from "src/std/AbiBufFitsTest.sol";

contract AbiBufFitsTestTest is BaseTest {
    AbiBufFitsTest solRef = new AbiBufFitsTest();
    address plankImpl = makeAddr("plank-implementation");

    function setUp() public {
        bytes memory plankCode = plank("src/std/abi_buf_fits_test.plk");
        vm.etch(plankImpl, plankCode);
    }

    function test_ceil32Bug_length33_buffer65() public {
        // length=33, padded data needs 64 bytes, so total = 32 + 64 = 96
        // but buffer is only 65 bytes (32 + 33)
        // buggy check: 32 + 33 = 65 <= 65 => true (wrong)
        // correct check: 32 + 64 = 96 > 65 => false
        bytes memory data = new bytes(65);
        assembly ("memory-safe") {
            mstore(add(data, 0x20), 33)
        }
        assertCallEq(address(solRef), plankImpl, data);
    }

    function test_fuzzing_ceil32Edge(uint256 len) public {
        uint256 size = 32 + (len % 128) + 1;
        bytes memory data = new bytes(size);
        assembly ("memory-safe") {
            mstore(add(data, 0x20), len)
        }
        assertCallEq(address(solRef), plankImpl, data);
    }
}
