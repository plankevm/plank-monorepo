// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @author philogy <https://github.com/philogy>
contract MallocBug {
    fallback() external {
        assembly {
            calldatacopy(0, 0, 32)
            return(0, 32)
        }
    }
}
