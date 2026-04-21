// SPDX-License-Identifier: MIT
pragma solidity =0.8.30;

contract AbiEncodePair {
    fallback() external payable {
        assembly ("memory-safe") {
            let a := calldataload(0x00)
            let b := calldataload(0x20)
            mstore(0x00, a)
            mstore(0x20, b)
            return(0x00, 0x40)
        }
    }
}
