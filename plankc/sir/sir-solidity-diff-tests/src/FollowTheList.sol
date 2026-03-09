// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @author philogy <https://github.com/philogy>
contract FollowTheList {
    fallback(bytes calldata input) external payable returns (bytes memory) {
        uint256[] memory nums = abi.decode(input, (uint256[]));

        assembly ("memory-safe") {
            if iszero(mload(nums)) {
                revert(0, 0)
            }
        }

        uint256 i = 0;
        while (i < nums.length) {
            i = nums[i];
        }

        assembly ("memory-safe") {
            mstore(0x00, i)
            return(0x00, 0x20)
        }
    }
}
