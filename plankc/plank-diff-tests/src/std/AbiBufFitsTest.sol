// SPDX-License-Identifier: MIT
pragma solidity =0.8.30;

contract AbiBufFitsTest {
    fallback() external payable {
        assembly ("memory-safe") {
            let size := calldatasize()
            if lt(size, 32) {
                mstore(0, 0)
                return(0, 32)
            }
            let len := calldataload(0)
            let padded := and(add(len, 31), not(31))
            if gt(add(32, padded), size) {
                mstore(0, 0)
                return(0, 32)
            }
            mstore(0, 1)
            return(0, 32)
        }
    }
}
