// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "test/BaseTest.sol";
import {IERC20} from "forge-std/interfaces/IERC20.sol";

/// forge-config: default.isolate = true
contract ERC20Benchmark is BaseTest {
    IERC20 token = IERC20(makeAddr("plank_test_token"));
    address initialSupplyHolder = makeAddr("initial_supply_holder");

    function setUp() public {
        vm.startPrank(initialSupplyHolder);
        token = IERC20(plankDeploy("src/examples/erc20.plk"));
        vm.stopPrank();
    }

    function test_deploy() public {
        bytes memory initcode = plank("src/examples/erc20.plk");
        vm.snapshotValue("erc20.initcode_size", initcode.length);
        address deployed = rawCreate(initcode);
        vm.snapshotGasLastCall("erc20.deploy");
        vm.snapshotValue("erc20.deployed_size", deployed.code.length);
    }

    function test_transferNonZeroToNonZero() public {
        address user = makeAddr("user");
        vm.prank(initialSupplyHolder);
        token.transfer(user, 1000);

        vm.prank(user);
        token.transfer(initialSupplyHolder, 20);
        vm.snapshotGasLastCall("erc20.transfer");
    }
}
