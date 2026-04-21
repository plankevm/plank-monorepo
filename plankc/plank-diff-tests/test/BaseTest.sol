// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Test} from "forge-std/Test.sol";

abstract contract BaseTest is Test {
    function deployCodeTo(address addr, bytes memory initcode) internal returns (bool success, bytes memory errdata) {
        vm.etch(addr, initcode);
        (success, errdata) = addr.call("");
        if (success) {
            vm.etch(addr, errdata);
            errdata = "";
        }
    }

    function plank(string memory sourcePath) internal returns (bytes memory) {
        string[] memory args = new string[](5);
        args[0] = "cargo";
        args[1] = "run";
        args[2] = "-p";
        args[3] = "plank";
        args[4] = "--";
        string[] memory fullArgs = new string[](8);
        fullArgs[0] = args[0];
        fullArgs[1] = args[1];
        fullArgs[2] = args[2];
        fullArgs[3] = args[3];
        fullArgs[4] = args[4];
        fullArgs[5] = "build";
        fullArgs[6] = sourcePath;
        fullArgs[7] = "--dep";
        // Grow by one more for the dep value
        string[] memory finalArgs = new string[](9);
        for (uint256 i = 0; i < 8; i++) {
            finalArgs[i] = fullArgs[i];
        }
        finalArgs[8] = string.concat("std=", vm.projectRoot(), "/../../std");
        return vm.ffi(finalArgs);
    }
}
