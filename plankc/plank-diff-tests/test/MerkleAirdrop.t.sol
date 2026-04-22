// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Test, Vm} from "forge-std/Test.sol";
import {BaseTest} from "./BaseTest.sol";
import {MerkleAirdrop} from "src/MerkleAirdrop.sol";

contract MerkleAirdropTest is BaseTest {
    MerkleAirdrop solRef;
    address plankImpl = makeAddr("plank-implementation");

    // Test accounts and amounts
    address alice = makeAddr("alice");
    address bob = makeAddr("bob");
    address charlie = makeAddr("charlie");
    address eve = makeAddr("eve");

    uint256 aliceAmt = 100;
    uint256 bobAmt = 200;
    uint256 charlieAmt = 300;
    uint256 eveAmt = 400;

    // Merkle tree leaves (sorted pairs at each level)
    bytes32 leafAlice;
    bytes32 leafBob;
    bytes32 leafCharlie;
    bytes32 leafEve;
    bytes32 nodeAB;
    bytes32 nodeCD;
    bytes32 root;

    function setUp() public {
        // Build merkle tree
        leafAlice = keccak256(abi.encodePacked(alice, aliceAmt));
        leafBob = keccak256(abi.encodePacked(bob, bobAmt));
        leafCharlie = keccak256(abi.encodePacked(charlie, charlieAmt));
        leafEve = keccak256(abi.encodePacked(eve, eveAmt));

        nodeAB = _hashPair(leafAlice, leafBob);
        nodeCD = _hashPair(leafCharlie, leafEve);
        root = _hashPair(nodeAB, nodeCD);

        // Deploy solidity reference with root as constructor arg
        solRef = new MerkleAirdrop(root);

        // Deploy plank implementation — pass root as constructor calldata
        bytes memory plankCode = plank("src/merkle_airdrop.plk");
        (bool success,) = deployCodeTo(plankImpl, plankCode, abi.encode(root));
        require(success, "plank deploy failed");
    }

    function _hashPair(bytes32 a, bytes32 b) internal pure returns (bytes32) {
        if (a <= b) return keccak256(abi.encodePacked(a, b));
        return keccak256(abi.encodePacked(b, a));
    }

    // --- helpers ---

    function assertCallEq(bytes memory data) internal {
        assertCallEqFrom(data, address(this));
    }

    function assertCallEqFrom(bytes memory data, address sender) internal {
        vm.startPrank(sender);

        vm.recordLogs();
        (bool refSucc, bytes memory refOut) = address(solRef).call(data);
        Vm.Log[] memory refLogs = vm.getRecordedLogs();

        vm.recordLogs();
        (bool plankSucc, bytes memory plankOut) = plankImpl.call(data);
        Vm.Log[] memory plankLogs = vm.getRecordedLogs();

        vm.stopPrank();

        assertEq(refSucc, plankSucc, "success mismatch");
        assertEq(refOut, plankOut, "output mismatch");
        assertEq(refLogs.length, plankLogs.length, "log count mismatch");
        for (uint256 i = 0; i < refLogs.length; i++) {
            assertEq(refLogs[i].data, plankLogs[i].data, "log data mismatch");
            assertEq(
                refLogs[i].topics.length,
                plankLogs[i].topics.length,
                "topic count mismatch"
            );
            for (uint256 j = 0; j < refLogs[i].topics.length; j++) {
                assertEq(refLogs[i].topics[j], plankLogs[i].topics[j], "topic mismatch");
            }
        }
    }

    // --- merkleRoot ---

    function test_merkleRoot() public {
        assertCallEq(abi.encodeWithSignature("merkleRoot()"));
    }

    // --- hasClaimed ---

    function test_hasClaimed_false() public {
        assertCallEq(abi.encodeWithSignature("hasClaimed(address)", alice));
    }

    // --- claim ---

    function test_claim_alice() public {
        bytes32[] memory proof = new bytes32[](2);
        proof[0] = leafBob;
        proof[1] = nodeCD;

        assertCallEq(
            abi.encodeWithSignature(
                "claim(address,uint256,bytes32[])",
                alice, aliceAmt, proof
            )
        );
    }

    function test_claim_bob() public {
        bytes32[] memory proof = new bytes32[](2);
        proof[0] = leafAlice;
        proof[1] = nodeCD;

        assertCallEq(
            abi.encodeWithSignature(
                "claim(address,uint256,bytes32[])",
                bob, bobAmt, proof
            )
        );
    }

    function test_claim_charlie() public {
        bytes32[] memory proof = new bytes32[](2);
        proof[0] = leafEve;
        proof[1] = nodeAB;

        assertCallEq(
            abi.encodeWithSignature(
                "claim(address,uint256,bytes32[])",
                charlie, charlieAmt, proof
            )
        );
    }

    // --- double claim reverts ---

    function test_double_claim_reverts() public {
        bytes32[] memory proof = new bytes32[](2);
        proof[0] = leafBob;
        proof[1] = nodeCD;

        // First claim succeeds
        assertCallEq(
            abi.encodeWithSignature(
                "claim(address,uint256,bytes32[])",
                alice, aliceAmt, proof
            )
        );

        // Second claim reverts
        assertCallEq(
            abi.encodeWithSignature(
                "claim(address,uint256,bytes32[])",
                alice, aliceAmt, proof
            )
        );
    }

    // --- hasClaimed after claim ---

    function test_hasClaimed_after_claim() public {
        bytes32[] memory proof = new bytes32[](2);
        proof[0] = leafBob;
        proof[1] = nodeCD;

        assertCallEq(
            abi.encodeWithSignature(
                "claim(address,uint256,bytes32[])",
                alice, aliceAmt, proof
            )
        );

        assertCallEq(abi.encodeWithSignature("hasClaimed(address)", alice));
    }

    // --- invalid proof reverts ---

    function test_invalid_proof_reverts() public {
        bytes32[] memory proof = new bytes32[](2);
        proof[0] = leafCharlie; // wrong sibling
        proof[1] = nodeCD;

        assertCallEq(
            abi.encodeWithSignature(
                "claim(address,uint256,bytes32[])",
                alice, aliceAmt, proof
            )
        );
    }

    // --- wrong amount reverts ---

    function test_wrong_amount_reverts() public {
        bytes32[] memory proof = new bytes32[](2);
        proof[0] = leafBob;
        proof[1] = nodeCD;

        assertCallEq(
            abi.encodeWithSignature(
                "claim(address,uint256,bytes32[])",
                alice, 999, proof
            )
        );
    }
}
