// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "./BaseTest.sol";
import {console} from "forge-std/console.sol";

/// @author philogy <https://github.com/philogy>
contract SimpleDataRetTest is BaseTest {
    address sirImpl = makeAddr("sir-implementation");
    bytes initcode = sir(abi.encode("src/simple_data_ret.sir"));

    function setUp() public {
        deployCodeTo(sirImpl, initcode);
    }

    function test_fuzzing_simpleDataRet(bytes calldata input) public {
        (bool success, bytes memory res) = sirImpl.call(input);

        console.logBytes(initcode);
        console.logBytes(sirImpl.code);

        assertTrue(success);
        assertEq(
            res,
            hex"8bd264b46c74b938c1d6859e756af47a05d95c39ae59b6f280469359afa7d5aa78aadf485b1ce22da227e4b2873a96e0d328f090c5341606d0b4ffa695f0618d00"
        );
    }

    function test_simpleDataRet() public {
        (bool success, bytes memory res) = sirImpl.call("");

        console.logBytes(initcode);
        console.logBytes(sirImpl.code);

        assertTrue(success);
        assertEq(
            res,
            hex"8bd264b46c74b938c1d6859e756af47a05d95c39ae59b6f280469359afa7d5aa78aadf485b1ce22da227e4b2873a96e0d328f090c5341606d0b4ffa695f0618d00"
        );
    }
}

