// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";
import {AbiDynamic} from "src/std/AbiDynamic.sol";

contract AbiDynamicTest is BaseTest {
    AbiDynamic solRef = new AbiDynamic();
    address plankImpl = makeAddr("plank-implementation");

    function setUp() public {
        bytes memory plankCode = plank("src/std/abi_dynamic.plk");
        vm.etch(plankImpl, plankCode);
    }

    function test_fuzzing_abiDynamic(uint256 id, bytes calldata data) public {
        bytes memory dataIn = abi.encode(id, data);
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool plankSucc, bytes memory plankOut) = plankImpl.call(dataIn);

        assertEq(refSucc, plankSucc, "different success");
        assertEq(refOut, plankOut, "different output data");
    }
}
