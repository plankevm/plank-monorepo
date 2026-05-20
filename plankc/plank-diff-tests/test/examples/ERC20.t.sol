// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "../BaseTest.sol";
import {ERC20} from "src/examples/ERC20.sol";
import {IERC20} from "forge-std/interfaces/IERC20.sol";

interface IERC20Permit {
    function DOMAIN_SEPARATOR() external view returns (bytes32);
    function nonces(address owner) external view returns (uint256);
    function permit(address owner, address spender, uint256 value, uint256 deadline, uint8 v, bytes32 r, bytes32 s)
        external;
}

contract ERC20Test is BaseTest {
    ERC20 solRef;
    ERC20 plankToken = ERC20(makeAddr("plank-implementation"));
    address minter = makeAddr("owner");

    address constant PERMIT2 = 0x000000000022D473030F116dDEE9F6B43aC78BA3;
    bytes32 constant PERMIT_TYPEHASH =
        keccak256("Permit(address owner,address spender,uint256 value,uint256 nonce,uint256 deadline)");

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

    function signPermit(
        IERC20Permit token,
        uint256 privateKey,
        address owner,
        address spender,
        uint256 amount,
        uint256 deadline
    ) internal view returns (uint8 v, bytes32 r, bytes32 s) {
        bytes32 structHash = keccak256(
            abi.encode(PERMIT_TYPEHASH, owner, spender, amount, token.nonces(owner), deadline)
        );
        bytes32 digest = keccak256(abi.encodePacked("\x19\x01", token.DOMAIN_SEPARATOR(), structHash));
        return vm.sign(privateKey, digest);
    }

    function signPermitData(
        IERC20Permit token,
        uint256 privateKey,
        address owner,
        address spender,
        uint256 amount,
        uint256 deadline
    ) internal view returns (bytes memory) {
        (uint8 v, bytes32 r, bytes32 s) = signPermit(token, privateKey, owner, spender, amount, deadline);
        return abi.encodeCall(IERC20Permit.permit, (owner, spender, amount, deadline, v, r, s));
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

    function test_domainSeparator() public {
        address commonTokenAddr = makeAddr("token");

        deployCodeTo(commonTokenAddr, type(ERC20).creationCode);
        bytes32 ds1 = IERC20Permit(commonTokenAddr).DOMAIN_SEPARATOR();

        deployCodeTo(commonTokenAddr, plank("src/examples/erc20.plk"));
        bytes32 ds2 = IERC20Permit(commonTokenAddr).DOMAIN_SEPARATOR();

        assertEq(ds1, ds2);
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

    function test_selfTransfer() public {
        uint256 amount = 2000;
        assertCallEqFrom(abi.encodeCall(IERC20.transfer, (minter, amount)), minter);

        assertEq(plankToken.balanceOf(minter), plankToken.totalSupply());
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

    function test_permit2ApproveMax() public {
        address owner = makeAddr("permit2-owner");
        assertCallEqFrom(abi.encodeCall(IERC20.approve, (PERMIT2, type(uint256).max)), owner);

        assertEq(plankToken.allowance(owner, PERMIT2), type(uint256).max);
    }

    function test_permit2TransferFrom() public {
        address recipient = makeAddr("recipient");
        uint256 amount = 2000;
        assertCallEqFrom(abi.encodeCall(IERC20.transferFrom, (minter, recipient, amount)), PERMIT2);

        assertEq(plankToken.balanceOf(minter), plankToken.totalSupply() - amount);
        assertEq(plankToken.balanceOf(recipient), amount);
        assertEq(plankToken.allowance(minter, PERMIT2), type(uint256).max);
    }

    function test_fuzzing_approve(address owner, address spender, uint256 amount) public {
        assertCallEqFrom(abi.encodeCall(IERC20.approve, (spender, amount)), owner);
    }

    function test_permit() public {
        uint256 privateKey = 0xA11CE;
        address owner = vm.addr(privateKey);
        address spender = makeAddr("spender");
        uint256 amount = 2000;
        uint256 deadline = block.timestamp;

        IERC20Permit solPermit = IERC20Permit(address(solRef));
        IERC20Permit plankPermit = IERC20Permit(address(plankToken));

        (uint8 v, bytes32 r, bytes32 s) = signPermit(solPermit, privateKey, owner, spender, amount, deadline);
        solPermit.permit(owner, spender, amount, deadline, v, r, s);

        (v, r, s) = signPermit(plankPermit, privateKey, owner, spender, amount, deadline);
        plankPermit.permit(owner, spender, amount, deadline, v, r, s);

        assertEq(plankToken.allowance(owner, spender), solRef.allowance(owner, spender));
        assertEq(plankPermit.nonces(owner), solPermit.nonces(owner));
    }

    function test_permitReplay() public {
        uint256 privateKey = 0xA11CE;
        address owner = vm.addr(privateKey);
        address spender = makeAddr("spender");
        uint256 amount = 2000;
        uint256 deadline = block.timestamp;

        bytes memory solPermit =
            signPermitData(IERC20Permit(address(solRef)), privateKey, owner, spender, amount, deadline);
        bytes memory plankPermit =
            signPermitData(IERC20Permit(address(plankToken)), privateKey, owner, spender, amount, deadline);

        (bool solSucc,) = address(solRef).call(solPermit);
        (bool plankSucc,) = address(plankToken).call(plankPermit);
        assertTrue(solSucc);
        assertTrue(plankSucc);

        bytes memory solOut;
        bytes memory plankOut;
        (solSucc, solOut) = address(solRef).call(solPermit);
        (plankSucc, plankOut) = address(plankToken).call(plankPermit);
        assertEq(solSucc, plankSucc, "success mismatch");
        assertEq(solOut, plankOut, "output mismatch");
    }

    function test_permitExpired() public {
        vm.warp(2);

        uint256 privateKey = 0xA11CE;
        address owner = vm.addr(privateKey);
        address spender = makeAddr("spender");
        uint256 amount = 2000;
        uint256 deadline = 1;

        bytes memory solPermit =
            signPermitData(IERC20Permit(address(solRef)), privateKey, owner, spender, amount, deadline);
        bytes memory plankPermit =
            signPermitData(IERC20Permit(address(plankToken)), privateKey, owner, spender, amount, deadline);

        (bool solSucc, bytes memory solOut) = address(solRef).call(solPermit);
        (bool plankSucc, bytes memory plankOut) = address(plankToken).call(plankPermit);
        assertEq(solSucc, plankSucc, "success mismatch");
        assertEq(solOut, plankOut, "output mismatch");
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

    function test_fuzzing_transferFromInsufficientBalance(uint256 amount) public {
        amount = bound(amount, plankToken.balanceOf(minter) + 1, type(uint256).max);

        address spender = makeAddr("spender");
        address recipient = makeAddr("recipient");

        assertCallEqFrom(abi.encodeCall(IERC20.approve, (spender, amount)), minter);
        assertCallEqFrom(abi.encodeCall(IERC20.transferFrom, (minter, recipient, amount)), spender);
        assertEq(plankToken.allowance(minter, spender), amount);
    }
}
