// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Vm} from "forge-std/Test.sol";
import {BaseTest} from "./BaseTest.sol";
import {ERC20} from "src/ERC20.sol";
import {IERC20} from "forge-std/interfaces/IERC20.sol";

contract ERC20Test is BaseTest {
    ERC20 solRef;
    ERC20 sirToken = ERC20(makeAddr("plank-implementation"));
    address minter = makeAddr("owner");

    address constant PERMIT2 = 0x000000000022D473030F116dDEE9F6B43aC78BA3;

    function setUp() public {
        solRef = new ERC20();

        bytes memory sirInitcode = sir(abi.encode("src/erc20.sir"));
        (bool initSucc,) = deployCodeTo(address(sirToken), sirInitcode);
        assertTrue(initSucc, "sir init failed");
    }

    function test_initialState() public view {
        assertEq(sirToken.balanceOf(minter), sirToken.totalSupply());
    }

    // --- helpers ---

    function assertCallEq(bytes memory data) internal {
        assertCallEq(address(solRef), address(sirToken), data);
    }

    function assertCallEqFrom(bytes memory data, address sender) internal {
        assertCallEqFrom(address(solRef), address(sirToken), data, sender);
    }

    function test_decimals() public {
        assertCallEq(abi.encodeCall(IERC20.decimals, ()));
    }

    function test_name() public {
        assertCallEq(abi.encodeCall(IERC20.name, ()));
    }

    function test_symbol() public {
        assertCallEq(abi.encodeCall(IERC20.symbol, ()));
    }

    function test_totalSupply() public {
        assertCallEq(abi.encodeCall(IERC20.totalSupply, ()));
    }

    function test_balanceOf_deployer() public {
        assertCallEq(abi.encodeCall(IERC20.balanceOf, (minter)));
    }

    function test_transfer() public {
        address recipient = makeAddr("recipient");
        uint256 amount = 2000;
        assertCallEqFrom(abi.encodeCall(IERC20.transfer, (recipient, amount)), minter);

        assertEq(sirToken.balanceOf(minter), sirToken.totalSupply() - amount);
        assertEq(sirToken.balanceOf(recipient), amount);
    }

    function test_selfTransfer() public {
        uint256 amount = 2000;
        assertCallEqFrom(abi.encodeCall(IERC20.transfer, (minter, amount)), minter);

        assertEq(sirToken.balanceOf(minter), sirToken.totalSupply());
    }

    function test_fuzzing_permit2Allowance(address owner) public {
        assertCallEq(abi.encodeCall(IERC20.allowance, (owner, PERMIT2)));
    }

    function test_fuzzing_insufficientBalance(uint256 amount) public {
        amount = bound(amount, sirToken.balanceOf(minter) + 1, type(uint256).max);

        address recipient = makeAddr("recipient");
        assertCallEqFrom(abi.encodeCall(IERC20.transfer, (recipient, amount)), minter);
    }

    function test_fuzzing_approvePermit2(address owner, uint256 amount) public {
        assertCallEqFrom(abi.encodeCall(IERC20.approve, (PERMIT2, amount)), owner);
    }

    function test_permit2ApproveMax() public {
        address owner = makeAddr("permit2-owner");
        assertCallEqFrom(abi.encodeCall(IERC20.approve, (PERMIT2, type(uint256).max)), owner);

        assertEq(sirToken.allowance(owner, PERMIT2), type(uint256).max);
    }

    function test_permit2TransferFrom() public {
        address recipient = makeAddr("recipient");
        uint256 amount = 2000;
        assertCallEqFrom(abi.encodeCall(IERC20.transferFrom, (minter, recipient, amount)), PERMIT2);

        assertEq(sirToken.balanceOf(minter), sirToken.totalSupply() - amount);
        assertEq(sirToken.balanceOf(recipient), amount);
        assertEq(sirToken.allowance(minter, PERMIT2), type(uint256).max);
    }

    function test_fuzzing_approve(address owner, address spender, uint256 amount) public {
        assertCallEqFrom(abi.encodeCall(IERC20.approve, (spender, amount)), owner);
    }

    function test_transferFrom() public {
        address spender = makeAddr("spender");
        address recipient = makeAddr("recipient");
        uint256 amount = 2000;

        assertCallEqFrom(abi.encodeCall(IERC20.approve, (spender, type(uint256).max)), minter);
        assertEq(sirToken.allowance(minter, spender), type(uint256).max, "allowance post set");
        assertCallEqFrom(abi.encodeCall(IERC20.transferFrom, (minter, recipient, amount)), spender);
        assertEq(sirToken.allowance(minter, spender), type(uint256).max, "allowance post transfer");
    }

    function test_fuzzing_transferFromAllowanceDecrease(uint256 amount, uint256 allowance) public {
        amount = bound(amount, 0, sirToken.balanceOf(minter));
        allowance = bound(allowance, amount, type(uint256).max - 1);

        address spender = makeAddr("spender");
        address recipient = makeAddr("recipient");

        assertCallEqFrom(abi.encodeCall(IERC20.approve, (spender, allowance)), minter);
        assertEq(sirToken.allowance(minter, spender), allowance);

        assertCallEqFrom(abi.encodeCall(IERC20.transferFrom, (minter, recipient, amount)), spender);
        assertEq(sirToken.allowance(minter, spender), allowance - amount);
    }

    function test_fuzzing_transferFromInsufficientAllowance(uint256 amount, uint256 allowance) public {
        amount = bound(amount, 1, sirToken.balanceOf(minter));
        allowance = bound(allowance, 0, amount - 1);

        address spender = makeAddr("spender");
        address recipient = makeAddr("recipient");

        assertCallEqFrom(abi.encodeCall(IERC20.approve, (spender, allowance)), minter);
        assertCallEqFrom(abi.encodeCall(IERC20.transferFrom, (minter, recipient, amount)), spender);
    }

    function test_fuzzing_transferFromInsufficientBalance(uint256 amount) public {
        amount = bound(amount, sirToken.balanceOf(minter) + 1, type(uint256).max);

        address spender = makeAddr("spender");
        address recipient = makeAddr("recipient");

        assertCallEqFrom(abi.encodeCall(IERC20.approve, (spender, amount)), minter);
        assertCallEqFrom(abi.encodeCall(IERC20.transferFrom, (minter, recipient, amount)), spender);
        assertEq(sirToken.allowance(minter, spender), amount);
    }
}
