// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";
import {ERC20} from "src/examples/ERC20.sol";
import {IERC20} from "forge-std/interfaces/IERC20.sol";

contract ERC20Test is BaseTest {
    ERC20 solRef;
    ERC20 plankToken = ERC20(makeAddr("plank-implementation"));
    address minter = makeAddr("owner");

    address constant PERMIT2 = 0x000000000022D473030F116dDEE9F6B43aC78BA3;

    function setUp() public {
        vm.startPrank(minter);
        solRef = new ERC20();

        bytes memory plankCode = plank("src/examples/erc20.plk");
        plankToken = ERC20(deployCode(plankCode));
        vm.stopPrank();
    }

    function test_initialState() public view {
        assertEq(plankToken.balanceOf(minter), plankToken.totalSupply());
    }

    // --- helpers ---

    function assertCallEq(bytes memory data) internal {
        assertCallEq(address(solRef), address(plankToken), data);
    }

    function assertCallEqFrom(bytes memory data, address sender) internal {
        assertCallEqFrom(address(solRef), address(plankToken), data, sender);
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

        assertEq(plankToken.balanceOf(minter), plankToken.totalSupply() - amount);
        assertEq(plankToken.balanceOf(recipient), amount);
    }

    function test_fuzzing_permit2Allowance(address owner) public {
        assertCallEq(abi.encodeCall(IERC20.allowance, (owner, PERMIT2)));
    }

    function test_fuzzing_insufficientBalance(uint256 amount) public {
        amount = bound(amount, plankToken.balanceOf(minter) + 1, type(uint256).max);

        address recipient = makeAddr("recipient");
        assertCallEqFrom(abi.encodeCall(IERC20.transfer, (recipient, amount)), minter);
    }

    function test_fuzzing_approvePermit2(address owner, uint256 amount) public {
        assertCallEqFrom(abi.encodeCall(IERC20.approve, (PERMIT2, amount)), owner);
    }

    function test_fuzzing_approve(address owner, address spender, uint256 amount) public {
        assertCallEqFrom(abi.encodeCall(IERC20.approve, (spender, amount)), owner);
    }

    function test_transferFrom() public {
        address spender = makeAddr("spender");
        address recipient = makeAddr("recipient");
        uint256 amount = 2000;

        assertCallEqFrom(abi.encodeCall(IERC20.approve, (spender, type(uint256).max)), minter);
        assertEq(plankToken.allowance(minter, spender), type(uint256).max, "allowance post set");
        assertCallEqFrom(abi.encodeCall(IERC20.transferFrom, (minter, recipient, amount)), spender);
        assertEq(plankToken.allowance(minter, spender), type(uint256).max, "allowance post transfer");
    }

    function test_fuzzing_transferFromAllowanceDecrease(uint256 amount, uint256 allowance) public {
        amount = bound(amount, 0, plankToken.balanceOf(minter));
        allowance = bound(allowance, amount, type(uint256).max - 1);

        address spender = makeAddr("spender");
        address recipient = makeAddr("recipient");

        assertCallEqFrom(abi.encodeCall(IERC20.approve, (spender, allowance)), minter);
        assertEq(plankToken.allowance(minter, spender), allowance);

        assertCallEqFrom(abi.encodeCall(IERC20.transferFrom, (minter, recipient, amount)), spender);
        assertEq(plankToken.allowance(minter, spender), allowance - amount);
    }

    function test_fuzzing_transferFromInsufficientAllowance(uint256 amount, uint256 allowance) public {
        amount = bound(amount, 1, plankToken.balanceOf(minter));
        allowance = bound(allowance, 0, amount - 1);

        address spender = makeAddr("spender");
        address recipient = makeAddr("recipient");

        assertCallEqFrom(abi.encodeCall(IERC20.approve, (spender, allowance)), minter);
        assertCallEqFrom(abi.encodeCall(IERC20.transferFrom, (minter, recipient, amount)), spender);
    }
}
