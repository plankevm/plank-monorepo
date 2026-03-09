// SPDX-License-Identifier: MIT
pragma solidity =0.8.30;

/// @author philogy <https://github.com/philogy>
contract SimpleAdd {
    fallback() external payable {
        assembly ("memory-safe") {
            let a := calldataload(0x00)
            let b := calldataload(0x20)
            let z := add(a, b)
            if gt(a, z) {
                revert(0, 0)
            }
            mstore(0x00, z)
            return(0x00, 0x20)
        }
    }
}
