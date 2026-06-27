// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";
import {Bit} from "src/std/Bit.sol";

contract BitTest is BaseTest {
    Bit solRef = new Bit();

    // Compiled with a pre-Osaka EVM version that lacks the CLZ opcode: exercises
    // the portable fallback in `std::bit::clz`.
    address plankFallback = makeAddr("plank-clz-fallback");

    function setUp() public {
        vm.etch(plankFallback, plank("src/std/bit_test.plk", "prague"));
    }

    function _clz(address impl, uint256 x) internal returns (uint256) {
        (bool ok, bytes memory out) = impl.call(abi.encode(x));
        assertTrue(ok, "call reverted");
        return abi.decode(out, (uint256));
    }

    function test_clz_knownValues() public {
        assertEq(_clz(plankFallback, 0), 256);
        assertEq(_clz(plankFallback, 1), 255);
        assertEq(_clz(plankFallback, 255), 248);
        assertEq(_clz(plankFallback, 256), 247);
        assertEq(_clz(plankFallback, 1 << 255), 0);
        assertEq(_clz(plankFallback, (1 << 160) - 1), 96);
        assertEq(_clz(plankFallback, type(uint256).max), 0);
    }

    function test_fuzzing_clzFallbackMatchesReference(uint256 x) public {
        (, bytes memory refOut) = address(solRef).call(abi.encode(x));
        assertEq(_clz(plankFallback, x), abi.decode(refOut, (uint256)));
    }

    // On Osaka (the default) the opcode path emits CLZ (0x1e, EIP-7939). We assert
    // the byte is present in the compiled output rather than executing it, since
    // the test EVM may not enable the opcode yet.
    function test_osakaCompilesToClzOpcode() public {
        bytes memory code = plank("src/std/bit_test.plk", "osaka");
        bool found;
        for (uint256 i = 0; i < code.length; i++) {
            if (uint8(code[i]) == 0x1e) {
                found = true;
                break;
            }
        }
        assertTrue(found, "CLZ opcode not emitted on Osaka EVM version");
    }
}
