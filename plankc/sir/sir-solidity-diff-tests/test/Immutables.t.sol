// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "./BaseTest.sol";

contract ImmutablesTest is BaseTest {
    address sirImpl = makeAddr("sir-implementation");

    function setUp() public {
        bytes memory sirCode = sir(abi.encode("src/immutables.sir"));
        (bool success, bytes memory errdata) = deployCodeTo(sirImpl, sirCode);
        assertTrue(success, string(errdata));
    }

    function test_immutables(bytes calldata input) public {
        (bool success, bytes memory out) = sirImpl.call(input);

        uint256 b1 = 0xab;
        uint256 b7 = 0x0102030405060f;
        uint256 b20 = uint256(uint160(address(this)));
        uint256 b32 = 0xff0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f;

        assertTrue(success);
        assertEq(out, abi.encode(b1, b7, b20, b32, b1, b7, b32, b7, b1));
    }
}
