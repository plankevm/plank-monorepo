// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract EventTest {
    event Transfer(address indexed from, address indexed to, uint256 amount);
    event NoIndex(uint256 a, bool b);
    event ThreeIndexed(uint256 indexed a, uint256 indexed b, uint256 indexed c, uint256 d);
    event DynIndexed(bytes indexed blob, bytes tail);
    event AnonFour(uint256 indexed a, uint256 indexed b, uint256 indexed c, uint256 indexed d, uint256 e) anonymous;

    event OneIndexed(address indexed who, uint256 value);

    event AnonZero(uint256 a, bool b) anonymous;
    event AnonOne(uint256 indexed a, uint256 b) anonymous;
    event AnonTwo(address indexed who, uint256 indexed b, uint256 c) anonymous;
    event AnonThree(uint256 indexed a, uint256 indexed b, uint256 indexed c, bool d) anonymous;

    event AllIndexed(address indexed who);
    event AnonAllIndexed(uint256 indexed a, uint256 indexed b) anonymous;

    event Flags(bytes32 indexed tag, bool indexed on, bytes32 salt, bool off);

    event MixedBag(uint256 indexed id, uint8 small, string text, uint32 wide, address who, bytes blob, uint160 huge);

    event UintIndexed(
        uint8 indexed tier, uint32 indexed rate, uint160 indexed payee, uint256 note, uint8 tag, uint248 max
    );

    event StrIndexed(string indexed key, string val);

    event BytesEdges(bytes empty, bytes exact, bytes over, bytes nine);
    event BytesEdgesIndexed(bytes indexed empty, bytes indexed exact, bytes indexed over, uint256 n);

    event OutOfOrderMarkers(uint256 indexed a, uint256 b, uint256 indexed c);

    event CodeRegion(bytes indexed tag, bytes body);

    fallback() external {
        emit Transfer(0x1111111111111111111111111111111111111111, 0x2222222222222222222222222222222222222222, 42);

        emit NoIndex(7, true);

        emit ThreeIndexed(1, 2, 3, 4);

        bytes memory blob = new bytes(5);
        emit DynIndexed(blob, blob);

        emit AnonFour(10, 20, 30, 40, 50);

        emit OneIndexed(0x3333333333333333333333333333333333333333, 99);

        emit AnonZero(111, false);

        emit AnonOne(222, 333);

        emit AnonTwo(0x4444444444444444444444444444444444444444, 444, 555);

        emit AnonThree(666, 777, 888, true);

        emit AllIndexed(0x6666666666666666666666666666666666666666);

        emit AnonAllIndexed(1234567, 7654321);

        emit Flags(
            0x00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff,
            true,
            0xfeedfacefeedfacefeedfacefeedfacefeedfacefeedfacefeedfacefeedface,
            false
        );

        emit MixedBag(
            1001,
            uint8(0xab),
            "plank",
            uint32(0xdeadbeef),
            0x5555555555555555555555555555555555555555,
            bytes("abc"),
            uint160(0xabcdef1234567890abcdef1234567890abcdef)
        );

        emit UintIndexed(
            uint8(0xff),
            uint32(0xdeadbeef),
            uint160(0xabcdef1234567890abcdef1234567890abcdef),
            7,
            uint8(0x01),
            uint248(0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
        );

        emit StrIndexed("key-with-some-length", "value string");

        bytes memory empty = "";
        bytes memory exact = "0123456789abcdef0123456789abcdef";
        bytes memory over = "0123456789abcdef0123456789abcdef01234567";
        bytes memory nine = "123456789";

        emit BytesEdges(empty, exact, over, nine);

        emit BytesEdgesIndexed(empty, exact, over, 1234);

        emit OutOfOrderMarkers(1, 2, 3);

        emit CodeRegion("code-tag", "code-body");
    }
}
