// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";
import {AbiNestedStruct} from "src/std/AbiNestedStruct.sol";

contract AbiNestedStructTest is BaseTest {
    AbiNestedStruct solRef = new AbiNestedStruct();
    address plankImpl = makeAddr("plank-implementation");

    function setUp() public {
        bytes memory plankCode = plank("src/std/abi_nested_struct.plk");
        vm.etch(plankImpl, plankCode);
    }

    function test_fuzzing_abiNestedStruct(uint256 x, bool flag, uint256 b, bool c) public {
        bytes memory dataIn = abi.encode(x, flag, b, c);
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool plankSucc, bytes memory plankOut) = plankImpl.call(dataIn);

        assertEq(refSucc, plankSucc, "different success");
        assertEq(refOut, plankOut, "different output data");
    }
}
