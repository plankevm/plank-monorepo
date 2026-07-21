// SPDX-License-Identifier: MIT
pragma solidity =0.8.30;

contract MinimalProxyFactory {
    event CloneCreated(address clone);

    function clone(address implementation) external returns (address result) {
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, 0x602c8060095f395ff3365f5f37602a5f5f5f365f730000000000000000000000)
            mstore(add(ptr, 0x15), shl(96, implementation))
            mstore(add(ptr, 0x29), 0x5af43d3d5f5f3e9257fd5bf30000000000000000000000000000000000000000)
            result := create(0, ptr, 0x35)
        }
        require(result != address(0));
        emit CloneCreated(result);
    }

    function cloneDeterministic(address implementation, bytes32 salt) external returns (address result) {
        assembly {
            let ptr := mload(0x40)
            mstore(ptr, 0x602c8060095f395ff3365f5f37602a5f5f5f365f730000000000000000000000)
            mstore(add(ptr, 0x15), shl(96, implementation))
            mstore(add(ptr, 0x29), 0x5af43d3d5f5f3e9257fd5bf30000000000000000000000000000000000000000)
            result := create2(0, ptr, 0x35, salt)
        }
        require(result != address(0));
        emit CloneCreated(result);
    }
}
