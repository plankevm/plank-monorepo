// SPDX-License-Identifier: MIT
pragma solidity =0.8.30;

contract AbiNestedStruct {
    fallback() external payable {
        uint256 len = msg.data.length;
        bytes memory result = new bytes(32 + len);
        assembly ("memory-safe") {
            mstore(add(result, 0x20), len)
            calldatacopy(add(add(result, 0x20), 32), 0, len)
        }
        assembly ("memory-safe") {
            return(add(result, 0x20), mload(result))
        }
    }
}
