// SPDX-License-Identifier: MIT
pragma solidity =0.8.30;

contract AbiNestedStruct {
    // Mirrors: struct Inner { x: u256, flag: bool }; struct Outer { a: Inner, b: u256, c: bool };
    // ABI layout (static): (uint256, bool, uint256, bool) = 4 × 32 bytes
    fallback() external payable {
        assembly ("memory-safe") {
            let ax := calldataload(0x00)
            let aflag := calldataload(0x20)
            let b := calldataload(0x40)
            let c := calldataload(0x60)
            let out := mload(0x40)
            mstore(out, ax)
            mstore(add(out, 0x20), aflag)
            mstore(add(out, 0x40), b)
            mstore(add(out, 0x60), c)
            return(out, 0x80)
        }
    }
}
