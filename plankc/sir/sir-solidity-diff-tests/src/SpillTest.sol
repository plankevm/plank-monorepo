// SPDX-License-Identifier: MIT
pragma solidity =0.8.30;

contract SpillTest {
    fallback() external payable {
        assembly ("memory-safe") {
            let s := 0
            for { let i := 0 } lt(i, 576) { i := add(i, 32) } {
                s := add(s, calldataload(i))
            }
            mstore(0x00, s)
            return(0x00, 0x20)
        }
    }
}
