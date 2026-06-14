// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";

contract RegionsTest is BaseTest {
    bytes internal constant TEST_BYTES = hex"00010203040506070809aabbccddeeff";

    address plankImpl;

    function setUp() public {
        plankImpl = deployCode(plank("src/std/regions_test.plk"));
    }

    function test_hashCodeLiteral() public {
        (bool success, bytes memory out) = plankImpl.call(abi.encodeWithSignature("hashCodeLiteral()"));
        assertTrue(success);
        assertEq(out.length, 32);
        assertEq(bytes32(out), keccak256(TEST_BYTES));
    }

    function test_copyCodeLiteral() public {
        (bool success, bytes memory out) = plankImpl.call(abi.encodeWithSignature("copyCodeLiteral()"));
        assertTrue(success);
        assertEq(out, TEST_BYTES);
    }

    function test_sliceCodeLiteral() public {
        (bool success, bytes memory out) =
            plankImpl.call(abi.encodeWithSignature("sliceCodeLiteral(uint256,uint256)", 10, 14));
        assertTrue(success);
        assertEq(out, hex"aabbccdd");
    }

    function test_sliceCodeLiteral_revertsOnInvalidRange() public {
        (bool startAfterEnd,) = plankImpl.call(abi.encodeWithSignature("sliceCodeLiteral(uint256,uint256)", 3, 2));
        assertFalse(startAfterEnd);

        (bool endOutOfBounds,) = plankImpl.call(abi.encodeWithSignature("sliceCodeLiteral(uint256,uint256)", 0, 17));
        assertFalse(endOutOfBounds);
    }

    function test_fuzzing_hashCalldata(bytes memory data) public {
        (bool success, bytes memory out) = plankImpl.call(abi.encodeWithSignature("hashCalldata(bytes)", data));
        assertTrue(success);
        assertEq(out.length, 32);
        assertEq(bytes32(out), keccak256(data));
    }

    function test_fuzzing_copyCalldata(bytes memory data) public {
        (bool success, bytes memory out) = plankImpl.call(abi.encodeWithSignature("copyCalldata(bytes)", data));
        assertTrue(success);
        assertEq(out, data);
    }

    function test_fuzzing_sliceCalldata(bytes memory data, uint256 start, uint256 end) public {
        start = bound(start, 0, data.length);
        end = bound(end, start, data.length);

        (bool success, bytes memory out) =
            plankImpl.call(abi.encodeWithSignature("sliceCalldata(bytes,uint256,uint256)", data, start, end));
        assertTrue(success);
        assertEq(out, slice(data, start, end));
    }

    function test_sliceCalldata_revertsOnInvalidRange() public {
        bytes memory data = hex"010203";

        (bool startAfterEnd,) =
            plankImpl.call(abi.encodeWithSignature("sliceCalldata(bytes,uint256,uint256)", data, 2, 1));
        assertFalse(startAfterEnd);

        (bool endOutOfBounds,) =
            plankImpl.call(abi.encodeWithSignature("sliceCalldata(bytes,uint256,uint256)", data, 0, 4));
        assertFalse(endOutOfBounds);
    }

    function slice(bytes memory data, uint256 start, uint256 end) internal pure returns (bytes memory out) {
        out = new bytes(end - start);
        for (uint256 i = 0; i < out.length; i++) {
            out[i] = data[start + i];
        }
    }
}
