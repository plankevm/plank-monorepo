// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";
import {StorageBytesTest} from "src/std/StorageBytesTest.sol";

contract StorageBytesTestTest is BaseTest {
    StorageBytesTest solRef;
    address plankImpl = makeAddr("plank-implementation");

    function setUp() public {
        solRef = new StorageBytesTest();

        bytes memory plankCode = plank("src/std/storage_bytes_test.plk");
        plankImpl = deployCode(plankCode);
    }

    function assertCallEq(bytes memory data) internal {
        assertCallEq(address(solRef), plankImpl, data);
    }

    function test_store_and_load_short() public {
        assertCallEq(abi.encodeWithSignature("store(bytes)", hex"deadbeef"));
        assertCallEq(abi.encodeWithSignature("load()"));
    }

    function test_store_and_load_empty() public {
        assertCallEq(abi.encodeWithSignature("store(bytes)", hex""));
        assertCallEq(abi.encodeWithSignature("load()"));
    }

    function test_store_and_load_exactly_32() public {
        assertCallEq(
            abi.encodeWithSignature(
                "store(bytes)", hex"0102030405060708091011121314151617181920212223242526272829303132"
            )
        );
        assertCallEq(abi.encodeWithSignature("load()"));
    }

    function test_store_and_load_multi_chunk() public {
        bytes memory bigData = new bytes(100);
        for (uint256 i = 0; i < 100; i++) {
            bigData[i] = bytes1(uint8(i));
        }
        assertCallEq(abi.encodeWithSignature("store(bytes)", bigData));
        assertCallEq(abi.encodeWithSignature("load()"));
    }

    function test_fuzzing_store_and_load(bytes calldata data) public {
        assertCallEq(abi.encodeWithSignature("store(bytes)", data));
        assertCallEq(abi.encodeWithSignature("load()"));
    }
}
