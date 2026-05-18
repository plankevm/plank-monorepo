// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "./BaseTest.sol";

contract ExternalCallTarget {
    fallback() external payable {
        uint256 word;
        assembly ("memory-safe") {
            word := calldataload(0)
        }
        bytes32 digest = keccak256(msg.data);
        bytes memory out = abi.encode(digest, msg.value);
        if (word == 0) {
            out = abi.encode(uint256(0xfeed), digest);
            assembly ("memory-safe") {
                revert(add(out, 0x20), mload(out))
            }
        }
        assembly ("memory-safe") {
            return(add(out, 0x20), mload(out))
        }
    }
}

contract ExternalCallsAndReturndataTest is BaseTest {
    address sirImpl = makeAddr("sir-implementation");
    ExternalCallTarget target = new ExternalCallTarget();

    function setUp() public {
        bytes memory sirCode = sir(abi.encode("src/external_calls_and_returndata.sir"));
        deployCodeTo(sirImpl, sirCode);
    }

    function test_fuzzing_externalCallsAndReturndata(uint256 callValue, uint256 payload1, uint256 payload2) public {
        vm.deal(sirImpl, callValue);
        bytes memory dataIn = abi.encode(address(target), callValue, payload1, payload2);
        (bool success, bytes memory out) = sirImpl.call(dataIn);

        assertTrue(success);
        assertEq(out, expected(callValue, payload1, payload2));
    }

    function expected(uint256 callValue, uint256 payload1, uint256 payload2) internal pure returns (bytes memory) {
        bytes memory callRet = targetOutput(callValue, payload1);
        bytes memory staticRet = targetOutput(0, payload2);
        return bytes.concat(
            abi.encode(payload1 != 0 ? uint256(1) : uint256(0), uint256(callRet.length)),
            callRet,
            abi.encode(payload2 != 0 ? uint256(1) : uint256(0), uint256(staticRet.length)),
            staticRet
        );
    }

    function targetOutput(uint256 callValue, uint256 payload) internal pure returns (bytes memory) {
        bytes memory data = abi.encode(payload);
        bytes32 digest = keccak256(data);
        if (payload == 0) {
            return abi.encode(uint256(0xfeed), digest);
        }
        return abi.encode(digest, callValue);
    }
}
