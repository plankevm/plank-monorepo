// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "test/BaseTest.sol";

contract RawEvmOpsTest is BaseTest {
    address timestampImpl = makeAddr("raw-evm-timestamp");
    address numberImpl = makeAddr("raw-evm-number");
    address chainidImpl = makeAddr("raw-evm-chainid");
    address basefeeImpl = makeAddr("raw-evm-basefee");
    address coinbaseImpl = makeAddr("raw-evm-coinbase");
    address gaspriceImpl = makeAddr("raw-evm-gasprice");
    address difficultyImpl = makeAddr("raw-evm-difficulty");
    address callerImpl = makeAddr("raw-evm-caller");
    address originImpl = makeAddr("raw-evm-origin");
    address callvalueImpl = makeAddr("raw-evm-callvalue");
    address selfbalanceImpl = makeAddr("raw-evm-selfbalance");
    address balanceImpl = makeAddr("raw-evm-balance");
    address blockhashImpl = makeAddr("raw-evm-blockhash");
    address blobhashImpl = makeAddr("raw-evm-blobhash");
    address sloadImpl = makeAddr("raw-evm-sload");
    bytes addressThisCode;

    function setUp() public {
        vm.etch(timestampImpl, plank("src/raw_evm_ops/timestamp.plk"));
        vm.etch(numberImpl, plank("src/raw_evm_ops/number.plk"));
        vm.etch(chainidImpl, plank("src/raw_evm_ops/chainid.plk"));
        vm.etch(basefeeImpl, plank("src/raw_evm_ops/basefee.plk"));
        vm.etch(coinbaseImpl, plank("src/raw_evm_ops/coinbase.plk"));
        vm.etch(gaspriceImpl, plank("src/raw_evm_ops/gasprice.plk"));
        vm.etch(difficultyImpl, plank("src/raw_evm_ops/difficulty.plk"));
        vm.etch(callerImpl, plank("src/raw_evm_ops/caller.plk"));
        vm.etch(originImpl, plank("src/raw_evm_ops/origin.plk"));
        vm.etch(callvalueImpl, plank("src/raw_evm_ops/callvalue.plk"));
        vm.etch(selfbalanceImpl, plank("src/raw_evm_ops/selfbalance.plk"));
        vm.etch(balanceImpl, plank("src/raw_evm_ops/balance.plk"));
        vm.etch(blockhashImpl, plank("src/raw_evm_ops/blockhash.plk"));
        vm.etch(blobhashImpl, plank("src/raw_evm_ops/blobhash.plk"));
        vm.etch(sloadImpl, plank("src/raw_evm_ops/sload.plk"));
        addressThisCode = plank("src/raw_evm_ops/address_this.plk");
    }

    function test_fuzzing_timestamp(uint256 timestamp) public {
        vm.warp(timestamp);
        assertReturns(timestampImpl, "", timestamp);
    }

    function test_fuzzing_number(uint256 number) public {
        vm.roll(number);
        assertReturns(numberImpl, "", number);
    }

    function test_fuzzing_chainid(uint64 chainid) public {
        vm.chainId(chainid);
        assertReturns(chainidImpl, "", chainid);
    }

    function test_fuzzing_basefee(uint64 basefee) public {
        vm.fee(basefee);
        assertReturns(basefeeImpl, "", basefee);
    }

    function test_fuzzing_coinbase(address coinbase) public {
        vm.coinbase(coinbase);
        assertReturns(coinbaseImpl, "", uint160(coinbase));
    }

    function test_fuzzing_gasprice(uint64 gasprice) public {
        vm.txGasPrice(gasprice);
        assertReturns(gaspriceImpl, "", gasprice);
    }

    function test_fuzzing_difficulty(uint256 difficulty) public {
        vm.prevrandao(difficulty);
        assertReturns(difficultyImpl, "", difficulty);
    }

    function test_fuzzing_caller(address caller) public {
        vm.prank(caller);
        assertReturns(callerImpl, "", uint160(caller));
    }

    function test_fuzzing_origin(address caller, address origin) public {
        vm.prank(caller, origin);
        assertReturns(originImpl, "", uint160(origin));
    }

    function test_fuzzing_callvalue(uint96 callvalue) public {
        vm.deal(address(this), callvalue);
        assertReturnsWithValue(callvalueImpl, "", callvalue, callvalue);
    }

    function test_fuzzing_selfbalance(uint256 balance) public {
        vm.deal(selfbalanceImpl, balance);
        assertReturns(selfbalanceImpl, "", balance);
    }

    function test_fuzzing_addressThis(address impl) public {
        vm.assume(uint160(impl) > 1024 && impl != address(vm));
        vm.etch(impl, addressThisCode);
        assertReturns(impl, "", uint160(impl));
    }

    function test_fuzzing_balance(address account, uint256 balance) public {
        vm.deal(account, balance);
        assertReturns(balanceImpl, abi.encode(uint256(uint160(account))), balance);
    }

    function test_fuzzing_blockhash(uint64 rawBlockNumber, bytes32 hash) public {
        uint256 blockNumber = bound(rawBlockNumber, 1, type(uint64).max - 1);
        vm.roll(blockNumber + 1);
        vm.setBlockhash(blockNumber, hash);
        assertReturns(blockhashImpl, abi.encode(blockNumber), uint256(hash));
    }

    function test_fuzzing_blobhash(uint256 rawIndex, bytes32 h0, bytes32 h1, bytes32 h2, bytes32 h3) public {
        bytes32[] memory hashes = new bytes32[](4);
        hashes[0] = h0;
        hashes[1] = h1;
        hashes[2] = h2;
        hashes[3] = h3;
        uint256 index = bound(rawIndex, 0, hashes.length - 1);
        vm.blobhashes(hashes);
        assertReturns(blobhashImpl, abi.encode(index), uint256(hashes[index]));
    }

    function test_fuzzing_sload(uint256 slot, uint256 value) public {
        vm.store(sloadImpl, bytes32(slot), bytes32(value));
        assertReturns(sloadImpl, abi.encode(slot), value);
    }

    function assertReturns(address impl, bytes memory data, uint256 expected) internal {
        assertReturnsWithValue(impl, data, 0, expected);
    }

    function assertReturnsWithValue(address impl, bytes memory data, uint256 value, uint256 expected) internal {
        (bool success, bytes memory out) = impl.call{value: value}(data);
        assertTrue(success);
        assertEq(out.length, 32);
        assertEq(abi.decode(out, (uint256)), expected);
    }
}
