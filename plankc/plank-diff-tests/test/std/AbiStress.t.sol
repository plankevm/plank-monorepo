// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";
import {AbiStressTest} from "src/std/AbiStressTest.sol";

struct Inner { uint256 x; bytes data; }
struct Outer { uint256 a; bool b; Inner inner; bytes c; }

contract AbiStressTestTest is BaseTest {
    AbiStressTest roundTripRef = new AbiStressTest();

    address plankRoundTrip = makeAddr("plank-round-trip");
    address plankBufFits = makeAddr("plank-buf-fits");

    function setUp() public {
        bytes memory roundTripCode = plank("src/std/abi_stress_test.plk");
        vm.etch(plankRoundTrip, roundTripCode);

        bytes memory bufFitsCode = plank("src/std/abi_buf_fits_test.plk");
        vm.etch(plankBufFits, bufFitsCode);
    }

    function _encodeOuter(Outer memory val) internal pure returns (bytes memory) {
        return abi.encode(val.a, val.b, val.inner, val.c);
    }

    // --- Buf fits tests (all assertions are in the plank code) ---

    function test_bufFits_allTypes() public {
        (bool succ, bytes memory ret) = plankBufFits.call("");
        assertTrue(succ, "buf fits assertions failed");
        assertEq(ret.length, 32);
        assertEq(abi.decode(ret, (uint256)), 1);
    }

    // --- Round-trip encode/decode tests ---

    function test_roundTrip_maxU256_true_smallData_emptyC() public {
        Outer memory val = Outer({
            a: type(uint256).max,
            b: true,
            inner: Inner({ x: 0, data: hex"deadbeef" }),
            c: new bytes(0)
        });
        assertCallEq(address(roundTripRef), plankRoundTrip, _encodeOuter(val));
    }

    function test_roundTrip_zero_false_33byteData_32byteC() public {
        bytes memory data33 = new bytes(33);
        for (uint256 i = 0; i < 33; i++) data33[i] = bytes1(uint8(i + 1));

        Outer memory val = Outer({
            a: 0,
            b: false,
            inner: Inner({ x: type(uint256).max, data: data33 }),
            c: new bytes(32)
        });
        assertCallEq(address(roundTripRef), plankRoundTrip, _encodeOuter(val));
    }

    function test_roundTrip_one_true_emptyData_1byteC() public {
        Outer memory val = Outer({
            a: 1,
            b: true,
            inner: Inner({ x: 1, data: new bytes(0) }),
            c: hex"ff"
        });
        assertCallEq(address(roundTripRef), plankRoundTrip, _encodeOuter(val));
    }

    function test_roundTrip_31byteData_63byteC() public {
        bytes memory data31 = new bytes(31);
        bytes memory c63 = new bytes(63);

        Outer memory val = Outer({
            a: 42,
            b: false,
            inner: Inner({ x: 1337, data: data31 }),
            c: c63
        });
        assertCallEq(address(roundTripRef), plankRoundTrip, _encodeOuter(val));
    }

    function test_fuzzing_roundTrip(
        uint256 a,
        bool b,
        uint256 x,
        bytes calldata data,
        bytes calldata c
    ) public {
        Outer memory val = Outer({
            a: a,
            b: b,
            inner: Inner({ x: x, data: data }),
            c: c
        });
        assertCallEq(address(roundTripRef), plankRoundTrip, _encodeOuter(val));
    }
}
