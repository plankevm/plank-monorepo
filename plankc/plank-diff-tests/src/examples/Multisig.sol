// SPDX-License-Identifier: MIT
pragma solidity =0.8.30;

contract Multisig {
    address[3] public owners;
    uint256 public constant threshold = 2;

    struct Transaction {
        address to;
        uint256 value;
        bytes data;
        bool executed;
    }

    Transaction[] public transactions;
    // txId => owner => confirmed
    mapping(uint256 => mapping(address => bool)) public confirmations;

    event Submit(uint256 indexed txId);
    event Confirm(address indexed owner, uint256 indexed txId);
    event Revoke(address indexed owner, uint256 indexed txId);
    event Execute(uint256 indexed txId);

    constructor(address[3] memory _owners) {
        owners = _owners;
    }

    modifier onlyOwner() {
        if (msg.sender != owners[0] && msg.sender != owners[1] && msg.sender != owners[2]) revert();
        _;
    }

    function submitTransaction(address to, uint256 value, bytes calldata data) external onlyOwner returns (uint256 txId) {
        txId = transactions.length;
        transactions.push(Transaction(to, value, data, false));
        emit Submit(txId);
    }

    function confirmTransaction(uint256 txId) external onlyOwner {
        if (txId >= transactions.length) revert();
        if (confirmations[txId][msg.sender]) revert();
        confirmations[txId][msg.sender] = true;
        emit Confirm(msg.sender, txId);
    }

    function revokeConfirmation(uint256 txId) external onlyOwner {
        if (txId >= transactions.length) revert();
        if (!confirmations[txId][msg.sender]) revert();
        confirmations[txId][msg.sender] = false;
        emit Revoke(msg.sender, txId);
    }

    function executeTransaction(uint256 txId) external onlyOwner {
        Transaction storage txn = transactions[txId];
        if (txn.executed) revert();
        if (getConfirmationCount(txId) < threshold) revert();
        txn.executed = true;
        (bool success,) = txn.to.call{value: txn.value}(txn.data);
        if (!success) revert();
        emit Execute(txId);
    }

    function getConfirmationCount(uint256 txId) public view returns (uint256 count) {
        for (uint256 i = 0; i < 3; i++) {
            if (confirmations[txId][owners[i]]) count++;
        }
    }

    function getTransaction(uint256 txId) external view returns (address to, uint256 value, bytes memory data, bool executed) {
        Transaction storage txn = transactions[txId];
        return (txn.to, txn.value, txn.data, txn.executed);
    }
}
