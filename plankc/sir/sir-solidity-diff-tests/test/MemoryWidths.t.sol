// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "./BaseTest.sol";

contract MemoryWidthsTest is BaseTest {
    bytes31 constant LEFT_1_PREFIX = 0x11111111111111111111111111111111111111111111111111111111111111;
    bytes30 constant LEFT_2_PREFIX = 0x222222222222222222222222222222222222222222222222222222222222;
    bytes29 constant LEFT_3_PREFIX = 0x3333333333333333333333333333333333333333333333333333333333;
    bytes16 constant LEFT_16_PREFIX = 0x44444444444444444444444444444444;
    bytes1 constant LEFT_31_PREFIX = 0x55;
    bytes32 constant RIGHT_1 = 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa;
    bytes32 constant RIGHT_2 = 0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb;
    bytes32 constant RIGHT_3 = 0xcccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc;
    bytes32 constant RIGHT_16 = 0xdddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd;
    bytes32 constant RIGHT_31 = 0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee;

    address sirImpl = makeAddr("sir-implementation");

    function setUp() public {
        bytes memory sirCode = sir(abi.encode("src/memory_widths.sir"));
        deployCodeTo(sirImpl, sirCode);
    }

    function test_fuzzing_memoryWidths(uint256 a, uint256 b, uint256 c, uint256 d, uint256 e) public {
        (bool success, bytes memory out) = sirImpl.call(abi.encode(a, b, c, d, e));

        assertTrue(success);
        assertEq(
            out,
            bytes.concat(
                abi.encode(uint256(uint8(a))),
                bytes.concat(LEFT_1_PREFIX, bytes1(uint8(a))),
                RIGHT_1,
                abi.encode(uint256(uint16(b))),
                bytes.concat(LEFT_2_PREFIX, bytes2(uint16(b))),
                RIGHT_2,
                abi.encode(uint256(uint24(c))),
                bytes.concat(LEFT_3_PREFIX, bytes3(uint24(c))),
                RIGHT_3,
                abi.encode(uint256(uint128(d))),
                bytes.concat(LEFT_16_PREFIX, bytes16(uint128(d))),
                RIGHT_16,
                abi.encode(uint256(uint248(e))),
                bytes.concat(LEFT_31_PREFIX, bytes31(uint248(e))),
                RIGHT_31
            )
        );
    }
}
