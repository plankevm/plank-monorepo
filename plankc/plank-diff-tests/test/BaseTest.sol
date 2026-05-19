// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Test, Vm} from "forge-std/Test.sol";
import {PlankDeployer, BuildOptions} from "plank-foundry-deployer/PlankDeployer.sol";

abstract contract BaseTest is Test, PlankDeployer {
    function deployCode(bytes memory initcode) internal returns (address addr) {
        addr = deployCode(initcode, "");
    }

    function deployCodeTo(address to, bytes memory initcode) internal {
        vm.etch(to, initcode);
        (bool success, bytes memory retdata) = to.call("");
        if (!success) {
            assembly ("memory-safe") {
                revert(add(retdata, 32), mload(retdata))
            }
        }
        vm.etch(to, retdata);
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

    function rawCreate(bytes memory initcode) internal returns (address) {
        return _deploy(initcode, 0);
    }

    function plankDeploy(string memory sourcePath) internal returns (address) {
        return _deploy(plank(sourcePath), 0);
    }

    function baseBuildOptions() internal view returns (BuildOptions memory) {
        return initBuildOptions().dependency("std", string.concat(vm.projectRoot(), "/../../std"));
    }

    function plankBuild(string memory sourcePath, BuildOptions memory options) internal returns (bytes memory) {
        string[] memory bin = new string[](1);
        bin[0] = "../target/debug/plank";
        return plankBuildFFI(bin, sourcePath, options);
    }

    function plank(string memory sourcePath) internal returns (bytes memory) {
        string memory backend = vm.envOr("PLANK_BACKEND", string("sir-debug"));
        string memory optimize = vm.envOr("PLANK_OPTIMIZE", string(""));

        BuildOptions memory options = baseBuildOptions().withBackend(backend);

        if (bytes(optimize).length != 0) {
            options = options.withOptimizations(optimize);
        } else {
            options = options.disableOptimizations();
        }

        return plankBuild(sourcePath, options);
    }
}
