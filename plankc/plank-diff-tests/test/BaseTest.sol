// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Test, Vm} from "forge-std/Test.sol";

abstract contract BaseTest is Test {
    function deployCode(bytes memory initcode) internal returns (address addr) {
        addr = deployCode(initcode, "");
    }

    function deployCode(bytes memory initcode, bytes memory args) internal returns (address addr) {
        initcode = bytes.concat(initcode, args);
        assembly ("memory-safe") {
            addr := create(0, add(initcode, 0x20), mload(initcode))
        }
        require(addr != address(0), "deploy failed");
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

    function plank(string memory sourcePath) internal returns (bytes memory) {
        string[] memory args = new string[](9);
        args[0] = "cargo";
        args[1] = "run";
        args[2] = "-p";
        args[3] = "plank";
        args[4] = "--";
        args[5] = "build";
        args[6] = sourcePath;
        args[7] = "--dep";
        args[8] = string.concat("std=", vm.projectRoot(), "/../../std");
        return vm.ffi(args);
    }
}
