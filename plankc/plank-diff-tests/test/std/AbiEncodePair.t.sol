// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";
import {AbiEncodePair} from "src/std/AbiEncodePair.sol";

contract AbiEncodePairTest is BaseTest {
    AbiEncodePair solRef = new AbiEncodePair();
    address plankImpl = makeAddr("plank-implementation");

    function setUp() public {
        bytes memory plankCode = plank("src/std/abi_encode_pair.plk");
        vm.etch(plankImpl, plankCode);
    }

    function test_fuzzing_abiEncodePair(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encode(a, b);
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool plankSucc, bytes memory plankOut) = plankImpl.call(dataIn);

        assertEq(refSucc, plankSucc, "different success");
        assertEq(refOut, plankOut, "different output data");
    }
}
