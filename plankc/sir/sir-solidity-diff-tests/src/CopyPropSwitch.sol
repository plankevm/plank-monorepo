// SPDX-License-Identifier: MIT
pragma solidity =0.8.30;

contract CopyPropSwitch {
    fallback() external payable {
        assembly ("memory-safe") {
            let selector := calldataload(0)
            let x := calldataload(32)
            let y := calldataload(64)
            let result := add(x, y)
            switch selector
            case 0 {
                mstore(0, x)
                return(0, 32)
            }
            case 1 {
                mstore(0, y)
                return(0, 32)
            }
            default {
                mstore(0, result)
                return(0, 32)
            }
        }
    }
}
