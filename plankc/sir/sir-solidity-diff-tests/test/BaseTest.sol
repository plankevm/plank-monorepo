// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Test} from "forge-std/Test.sol";

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

        totalArgs = 5 + sirArgs.length;
        if (releaseBackendEnabled) totalArgs++;

        string[] memory args = new string[](totalArgs);

        uint256 argIdx = 0;
        string[5] memory runSir = ["cargo", "run", "-p", "sir-cli", "--"];
        for (uint256 i = 0; i < runSir.length; i++) {
            args[argIdx++] = runSir[i];
        }
        if (releaseBackendEnabled) {
            args[argIdx++] = "--release";
        }
        for (uint256 i = 0; i < sirArgs.length; i++) {
            args[argIdx++] = sirArgs[i];
        }

        return vm.ffi(args);
    }
}
