// SPDX-License-Identifier: MIT
pragma solidity =0.8.30;

contract MerkleAirdrop {
    bytes32 public merkleRoot;
    mapping(address => bool) public hasClaimed;

    event Claimed(address indexed account, uint256 amount);

    constructor(bytes32 root) {
        merkleRoot = root;
    }

    function claim(address account, uint256 amount, bytes32[] calldata proof) external {
        if (hasClaimed[account]) revert();

        bytes32 leaf = keccak256(abi.encodePacked(account, amount));
        bytes32 node = leaf;
        for (uint256 i = 0; i < proof.length; i++) {
            bytes32 proofElement = proof[i];
            if (node <= proofElement) {
                node = keccak256(abi.encodePacked(node, proofElement));
            } else {
                node = keccak256(abi.encodePacked(proofElement, node));
            }
        }

        if (node != merkleRoot) revert();

        hasClaimed[account] = true;
        emit Claimed(account, amount);
    }
}
