// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "test/BaseTest.sol";
import {IERC20} from "forge-std/interfaces/IERC20.sol";
import {ERC20} from "src/examples/ERC20.sol";

/// forge-config: default.isolate = true
abstract contract ERC20BenchmarkBase is BaseTest {
    IERC20 token = IERC20(makeAddr("plank_test_token"));
    address initialSupplyHolder = makeAddr("initial_supply_holder");

    function group() internal pure virtual returns (string memory);
    function compile() internal virtual returns (bytes memory);

    function setUp() public {
        vm.startPrank(initialSupplyHolder);
        token = IERC20(rawCreate(compile()));
        vm.stopPrank();
    }

    function test_deploy() public {
        bytes memory initcode = compile();
        vm.snapshotValue(group(), "erc20.initcode_size", initcode.length);
        address deployed = rawCreate(initcode);
        vm.snapshotGasLastCall(group(), "erc20.deploy");
        vm.snapshotValue(group(), "erc20.deployed_size", deployed.code.length);
    }

    function test_transferNonZeroToNonZero() public {
        address user = makeAddr("user");
        vm.prank(initialSupplyHolder);
        token.transfer(user, 1000);

        vm.prank(user);
        token.transfer(initialSupplyHolder, 20);
        vm.snapshotGasLastCall(group(), "erc20.transfer");
    }

    function test_balanceOf() public {
        address user = makeAddr("user");
        token.balanceOf(user);
        vm.snapshotGasLastCall(group(), "erc20.balance_of");
    }
}

contract ERC20PlankRelease is ERC20BenchmarkBase {
    function group() internal pure override returns (string memory) {
        return "plank-release";
    }

    function compile() internal override returns (bytes memory) {
        return
            plankBuild(
                "src/examples/erc20.plk", baseBuildOptions().withBackend("sir-release").withOptimizations("csud")
            );
    }
}

contract ERC20Solady is ERC20BenchmarkBase {
    function group() internal pure override returns (string memory) {
        return "solady";
    }

    function compile() internal pure override returns (bytes memory) {
        return type(ERC20).creationCode;
    }
}
