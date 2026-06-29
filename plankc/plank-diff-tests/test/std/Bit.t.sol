// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";
import {Bit} from "src/std/Bit.sol";

contract BitTest is BaseTest {
    Bit solRef = new Bit();

    address plankFallback = makeAddr("plank-clz-fallback");
    address plankOsaka = makeAddr("plank-clz-osaka");

    function setUp() public {
        vm.etch(plankFallback, plank("src/std/bit_test.plk", "prague"));
        vm.etch(plankOsaka, plank("src/std/bit_test.plk", "osaka"));
    }

    function _clz(address impl, uint256 x) internal returns (uint256) {
        (bool ok, bytes memory out) = impl.call(abi.encode(x));
        assertTrue(ok, "call reverted");
        return abi.decode(out, (uint256));
    }

    function _assertKnownValues(address impl) internal {
        assertEq(_clz(impl, 0), 256);
        assertEq(_clz(impl, 1), 255);
        assertEq(_clz(impl, 255), 248);
        assertEq(_clz(impl, 256), 247);
        assertEq(_clz(impl, 1 << 255), 0);
        assertEq(_clz(impl, (1 << 160) - 1), 96);
        assertEq(_clz(impl, type(uint256).max), 0);
    }

    function test_clz_knownValues_fallback() public {
        _assertKnownValues(plankFallback);
    }

    function test_clz_knownValues_osaka() public {
        _assertKnownValues(plankOsaka);
    }

    function test_fuzzing_clzFallbackMatchesReference(uint256 x) public {
        (bool ok, bytes memory refOut) = address(solRef).call(abi.encode(x));
        assertTrue(ok, "ref call reverted");
        assertEq(_clz(plankFallback, x), abi.decode(refOut, (uint256)));
    }

    function test_fuzzing_clzOsakaMatchesReference(uint256 x) public {
        (bool ok, bytes memory refOut) = address(solRef).call(abi.encode(x));
        assertTrue(ok, "ref call reverted");
        assertEq(_clz(plankOsaka, x), abi.decode(refOut, (uint256)));
    }
}
