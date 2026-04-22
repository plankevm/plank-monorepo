// SPDX-License-Identifier: MIT
pragma solidity =0.8.30;

contract AbiDynamic {
    // Mirrors: struct WithBytes { id: u256, data: membytes };
    // ABI layout (dynamic): (uint256, bytes) — head has id + offset, tail has length + padded data
    fallback() external payable {
        assembly ("memory-safe") {
            let size := calldatasize()
            let out := mload(0x40)
            calldatacopy(out, 0, size)
            return(out, size)
        }
    }
}
