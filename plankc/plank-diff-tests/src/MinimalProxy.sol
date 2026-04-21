// SPDX-License-Identifier: MIT
pragma solidity =0.8.30;

contract MinimalProxyFactory {
    event CloneCreated(address clone);

    function clone(address implementation) external returns (address result) {
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, 0x3d602d80600a3d3981f3363d3d373d3d3d363d73000000000000000000000000)
            mstore(add(ptr, 0x14), shl(96, implementation))
            mstore(add(ptr, 0x28), 0x5af43d82803e903d91602b57fd5bf30000000000000000000000000000000000)
            result := create(0, ptr, 0x37)
        }
        require(result != address(0));
        emit CloneCreated(result);
    }

    function cloneDeterministic(address implementation, bytes32 salt) external returns (address result) {
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, 0x3d602d80600a3d3981f3363d3d373d3d3d363d73000000000000000000000000)
            mstore(add(ptr, 0x14), shl(96, implementation))
            mstore(add(ptr, 0x28), 0x5af43d82803e903d91602b57fd5bf30000000000000000000000000000000000)
            result := create2(0, ptr, 0x37, salt)
        }
        require(result != address(0));
        emit CloneCreated(result);
    }
}
