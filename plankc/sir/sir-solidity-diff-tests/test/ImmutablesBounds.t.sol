// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "./BaseTest.sol";

/// @dev `src/immutables_bounds.sir` copies the runtime to the fixed address `RUNTIME_PTR` and sets
/// every immutable to the first calldata word. With the release backend (O2) its `b2` placeholder is
/// in the last 32 bytes of the runtime, so patching it with a whole word would write past the copy.
contract ImmutablesBoundsTest is BaseTest {
    uint64 constant RUNTIME_PTR = 0x10000;
    uint256 constant PLACEHOLDER_BYTES = 1 + 2 * 31 + 32 + 2 * 2;

    bytes initcode;
    address zeroed = makeAddr("zeroed");
    address ones = makeAddr("ones");

    function setUp() public {
        initcode = sir(abi.encode("src/immutables_bounds.sir"));
        deployWith(zeroed, abi.encode(0));
        deployWith(ones, abi.encode(type(uint256).max));
    }

    function deployWith(address addr, bytes memory input) internal {
        vm.etch(addr, initcode);
        (bool success, bytes memory runtime) = addr.call(input);
        assertTrue(success);
        vm.etch(addr, runtime);
    }

    function test_onlyPlaceholderBytesPatched() public view {
        bytes memory zeroedCode = zeroed.code;
        bytes memory onesCode = ones.code;
        assertEq(zeroedCode.length, onesCode.length);

        uint256 patched = 0;
        for (uint256 i = 0; i < zeroedCode.length; i++) {
            if (zeroedCode[i] == onesCode[i]) continue;
            assertEq(zeroedCode[i], bytes1(0x00));
            assertEq(onesCode[i], bytes1(0xff));
            patched++;
        }
        assertEq(patched, PLACEHOLDER_BYTES);
    }

    function test_oversizedValueTruncatedToSize() public {
        (bool success, bytes memory out) = ones.call("");

        assertTrue(success);
        assertEq(
            out,
            abi.encode(
                type(uint8).max,
                type(uint248).max,
                type(uint256).max,
                type(uint248).max,
                type(uint16).max,
                type(uint16).max
            )
        );
    }

    function test_noMemoryWritesPastRuntimeCopy() public {
        address fresh = makeAddr("fresh");
        vm.etch(fresh, initcode);
        vm.expectSafeMemoryCall(0x60, RUNTIME_PTR + uint64(zeroed.code.length));
        (bool success,) = fresh.call(abi.encode(type(uint256).max));
        assertTrue(success);
    }
}
