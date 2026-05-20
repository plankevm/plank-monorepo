// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Test, Vm} from "forge-std/Test.sol";

/// @author philogy <https://github.com/philogy>
abstract contract BaseTest is Test {
    function deployCodeTo(address addr, bytes memory initcode) internal returns (bool success, bytes memory errdata) {
        vm.etch(addr, initcode);
        (success, errdata) = addr.call("");
        if (success) {
            vm.etch(addr, errdata);
            errdata = "";
        }
    }

    function sir(bytes memory encodedSirArgs) internal returns (bytes memory) {
        bool releaseBackendEnabled;
        {
            string memory releaseBackendEnabledStr = vm.envOr(string("SIR_RELEASE_BACKEND"), string("false"));
            bytes32 releaseBackendEnabledHash = keccak256(bytes(releaseBackendEnabledStr));
            if (releaseBackendEnabledHash == keccak256("true") || releaseBackendEnabledHash == keccak256("1")) {
                releaseBackendEnabled = true;
            } else if (releaseBackendEnabledHash == keccak256("false") || releaseBackendEnabledHash == keccak256("0")) {
                releaseBackendEnabled = false;
            } else {
                revert(string.concat("unexpected/invalid SIR_RELEASE_BACKEND value '", releaseBackendEnabledStr, "'"));
            }
        }

        uint256 totalArgs;
        assembly ("memory-safe") {
            let firstOffset := mload(add(encodedSirArgs, 0x20))
            totalArgs := div(firstOffset, 0x20)
        }
        string[] memory sirArgs =
            abi.decode(bytes.concat(bytes32(uint256(0x20)), bytes32(totalArgs), encodedSirArgs), (string[]));

        string[] memory args = new string[](300);

        uint256 argLen = 0;
        args[argLen++] = "../../target/debug/sir";
        if (releaseBackendEnabled) {
            args[argLen++] = "--release";
        }
        for (uint256 i = 0; i < sirArgs.length; i++) {
            args[argLen++] = sirArgs[i];
        }
        assembly ("memory-safe") {
            mstore(args, argLen)
        }

        return vm.ffi(args);
    }

    function assertCallEq(address ref, address impl, bytes memory data) internal {
        assertCallEqFrom(ref, impl, data, address(this));
    }

    function assertCallEqFrom(address ref, address impl, bytes memory data, address sender) internal {
        vm.startPrank(sender);

        vm.recordLogs();
        (bool refSucc, bytes memory refOut) = ref.call(data);
        Vm.Log[] memory refLogs = vm.getRecordedLogs();

        vm.recordLogs();
        (bool plankSucc, bytes memory plankOut) = impl.call(data);
        Vm.Log[] memory plankLogs = vm.getRecordedLogs();

        vm.stopPrank();

        assertEq(refSucc, plankSucc, "success mismatch");
        assertEq(refOut, plankOut, "output mismatch");
        assertEq(refLogs.length, plankLogs.length, "log count mismatch");
        for (uint256 i = 0; i < refLogs.length; i++) {
            assertEq(refLogs[i].data, plankLogs[i].data, "log data mismatch");
            assertEq(refLogs[i].topics.length, plankLogs[i].topics.length, "topic count mismatch");
            for (uint256 j = 0; j < refLogs[i].topics.length; j++) {
                assertEq(refLogs[i].topics[j], plankLogs[i].topics[j], "topic mismatch");
            }
        }
    }
}
