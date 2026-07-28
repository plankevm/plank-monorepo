// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract AbiStringTest {
    fallback(bytes calldata input) external returns (bytes memory) {
        (uint256 a, string memory s) = abi.decode(input, (uint256, string));
        return abi.encode(a, s);
    }
}
