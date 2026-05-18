// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "./BaseTest.sol";

contract AllocatorLayoutTest is BaseTest {
    address sirImpl = makeAddr("sir-implementation");

    function setUp() public {
        bytes memory sirCode = sir(abi.encode("src/allocator_layout.sir"));
        deployCodeTo(sirImpl, sirCode);
    }

    function test_fuzzing_allocatorLayout(
        uint256 selector,
        uint256 valStatic,
        uint256 valStaticAny,
        uint256 valDyn0,
        uint256 valDyn1,
        uint256 valDynAny
    ) public {
        selector = bound(selector, 0, 3);
        selector = (0xa0 + 0x11 * selector);

        (bool success, bytes memory out) =
            sirImpl.call(abi.encode(selector, valStatic, valStaticAny, valDyn0, valDyn1, valDynAny));

        assertTrue(success);
        assertEq(
            out,
            bytes.concat(
                abi.encode(uint256(0), uint256(0), uint256(0)),
                abi.encode(valStatic, valStaticAny, valDyn0),
                abi.encode(valDyn1, valDynAny, selector)
            )
        );
    }
}
