// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";
import {AbiNestedStruct} from "src/std/AbiNestedStruct.sol";

struct Level0 { uint256 x; bytes data; }
struct Level1 { Level0 inner; bool flag; }
struct Level2 { Level1 inner; uint256 y; bytes suffix; }

contract AbiNestedStructTest is BaseTest {
    AbiNestedStruct solRef = new AbiNestedStruct();
    address plankImpl = makeAddr("plank-implementation");

    function setUp() public {
        bytes memory plankCode = plank("src/std/abi_nested_struct.plk");
        vm.etch(plankImpl, plankCode);
    }

    function _encode(Level2 memory val) internal pure returns (bytes memory) {
        return abi.encode(val.inner, val.y, val.suffix);
    }

    function test_fuzzing_abiNestedStruct(
        uint256 x,
        bytes calldata data,
        bool flag,
        uint256 y,
        bytes calldata suffix
    ) public {
        Level2 memory val = Level2({
            inner: Level1({
                inner: Level0({ x: x, data: data }),
                flag: flag
            }),
            y: y,
            suffix: suffix
        });
        assertCallEq(address(solRef), plankImpl, _encode(val));
    }
}
