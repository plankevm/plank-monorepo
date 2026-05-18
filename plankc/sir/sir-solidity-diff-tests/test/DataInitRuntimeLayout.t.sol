// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "./BaseTest.sol";

contract DataInitRuntimeLayoutTest is BaseTest {
    address sirImpl = makeAddr("sir-implementation");

    function setUp() public {
        bytes memory sirCode = sir(abi.encode("src/data_init_runtime_layout.sir"));
        (bool success, bytes memory errdata) = deployCodeTo(sirImpl, sirCode);
        assertTrue(success, string(errdata));
    }

    function test_dataInitRuntimeLayout(bytes calldata input) public {
        (bool success, bytes memory out) = sirImpl.call(input);

        assertTrue(success);
        assertEq(
            out,
            hex"aabbccddeeff001122334455667788999887766554433221100ffeeddccbbaa0000102030405060708090a0b0c0d0e0f10f0efeeedecebeae9e8e7e6e5e4e300"
        );
    }
}
