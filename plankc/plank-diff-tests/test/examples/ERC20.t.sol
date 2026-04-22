// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";
import {ERC20} from "src/examples/ERC20.sol";

contract ERC20Test is BaseTest {
    ERC20 solRef;
    address plankImpl = makeAddr("plank-implementation");

    function setUp() public {
        solRef = new ERC20();

        bytes memory plankCode = plank("src/examples/erc20.plk");
        plankImpl = deployCode(plankCode);
    }

    // --- helpers ---

    function assertCallEq(bytes memory data) internal {
        assertCallEq(address(solRef), plankImpl, data);
    }

    function assertCallEqFrom(bytes memory data, address sender) internal {
        assertCallEqFrom(address(solRef), plankImpl, data, sender);
    }

    // --- view functions ---

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

    // --- transfer ---

    function test_fuzzing_transfer(address to, uint256 amount) public {
        assertCallEq(abi.encodeWithSignature("transfer(address,uint256)", to, amount));
    }

    // --- approve ---

    function test_fuzzing_approve(address spender, uint256 amount) public {
        assertCallEq(abi.encodeWithSignature("approve(address,uint256)", spender, amount));
    }

    // --- transferFrom (multi-step) ---

    function test_fuzzing_transferFrom(address from, address to, uint256 approveAmt, uint256 transferAmt) public {
        vm.assume(from != address(this));
        approveAmt = bound(approveAmt, 0, 1000000);
        transferAmt = bound(transferAmt, 0, approveAmt);

        // give `from` some tokens
        assertCallEq(abi.encodeWithSignature("transfer(address,uint256)", from, approveAmt));

        // `from` approves this contract
        assertCallEqFrom(abi.encodeWithSignature("approve(address,uint256)", address(this), approveAmt), from);

        // this contract calls transferFrom
        assertCallEq(abi.encodeWithSignature("transferFrom(address,address,uint256)", from, to, transferAmt));
    }
}
