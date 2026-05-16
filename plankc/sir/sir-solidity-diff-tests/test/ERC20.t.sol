// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Vm} from "forge-std/Test.sol";
import {BaseTest} from "./BaseTest.sol";
import {ERC20} from "src/ERC20.sol";

contract ERC20Test is BaseTest {
    ERC20 solRef;
    address sirImpl = makeAddr("sir-erc20-implementation");

    function setUp() public {
        solRef = new ERC20();

        bytes memory sirInitcode = sir(abi.encode("src/erc20.sir"));
        (bool initSucc,) = deployCodeTo(sirImpl, sirInitcode);
        assertTrue(initSucc, "sir init failed");
    }

    function assertCallEq(bytes memory data) internal {
        assertCallEqFrom(data, address(this));
    }

    function assertCallEqFrom(bytes memory data, address sender) internal {
        vm.startPrank(sender);

        vm.recordLogs();
        (bool refSucc, bytes memory refOut) = address(solRef).call(data);
        Vm.Log[] memory refLogs = vm.getRecordedLogs();

        vm.recordLogs();
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(data);
        Vm.Log[] memory sirLogs = vm.getRecordedLogs();

        vm.stopPrank();

        assertEq(refSucc, sirSucc, "success mismatch");
        assertEq(refOut, sirOut, "output mismatch");
        assertEq(refLogs.length, sirLogs.length, "log count mismatch");
        for (uint256 i = 0; i < refLogs.length; i++) {
            assertEq(refLogs[i].data, sirLogs[i].data, "log data mismatch");
            assertEq(refLogs[i].topics.length, sirLogs[i].topics.length, "topic count mismatch");
            for (uint256 j = 0; j < refLogs[i].topics.length; j++) {
                assertEq(refLogs[i].topics[j], sirLogs[i].topics[j], "topic mismatch");
            }
        }
    }

    function test_decimals() public {
        assertCallEq(abi.encodeWithSignature("decimals()"));
    }

    function test_totalSupply() public {
        assertCallEq(abi.encodeWithSignature("totalSupply()"));
    }

    function test_balanceOf_deployer() public {
        assertCallEq(abi.encodeWithSignature("balanceOf(address)", address(this)));
    }

    function test_fuzzing_balanceOf(address who) public {
        assertCallEq(abi.encodeWithSignature("balanceOf(address)", who));
    }

    function test_fuzzing_allowance(address owner, address spender) public {
        assertCallEq(abi.encodeWithSignature("allowance(address,address)", owner, spender));
    }

    function test_fuzzing_transfer(address to, uint256 amount) public {
        assertCallEq(abi.encodeWithSignature("transfer(address,uint256)", to, amount));
    }

    function test_fuzzing_approve(address spender, uint256 amount) public {
        assertCallEq(abi.encodeWithSignature("approve(address,uint256)", spender, amount));
    }

    function test_fuzzing_transferFrom(address from, address to, uint256 approveAmt, uint256 transferAmt) public {
        vm.assume(from != address(this));
        approveAmt = bound(approveAmt, 0, 1000000);
        transferAmt = bound(transferAmt, 0, approveAmt);

        assertCallEq(abi.encodeWithSignature("transfer(address,uint256)", from, approveAmt));
        assertCallEqFrom(abi.encodeWithSignature("approve(address,uint256)", address(this), approveAmt), from);
        assertCallEq(abi.encodeWithSignature("transferFrom(address,address,uint256)", from, to, transferAmt));
    }
}
