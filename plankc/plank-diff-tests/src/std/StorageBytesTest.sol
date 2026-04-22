// SPDX-License-Identifier: MIT
pragma solidity =0.8.30;

contract StorageBytesTest {
    bytes data;

    function store(bytes calldata _data) external {
        data = _data;
    }

    function load() external view returns (bytes memory) {
        return data;
    }
}
