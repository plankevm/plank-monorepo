// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "test/BaseTest.sol";
import {IERC20} from "forge-std/interfaces/IERC20.sol";
import {ERC20} from "src/examples/ERC20.sol";

interface IERC20Permit {
    function DOMAIN_SEPARATOR() external view returns (bytes32);
    function nonces(address owner) external view returns (uint256);
    function permit(address owner, address spender, uint256 value, uint256 deadline, uint8 v, bytes32 r, bytes32 s)
        external;
}

abstract contract ERC20BenchmarkBase is BaseTest {
    IERC20 token = IERC20(makeAddr("plank_test_token"));
    address initialSupplyHolder = makeAddr("initial_supply_holder");
    bytes32 constant PERMIT_TYPEHASH =
        keccak256("Permit(address owner,address spender,uint256 value,uint256 nonce,uint256 deadline)");

    function group() internal pure virtual returns (string memory);
    function compile() internal virtual returns (bytes memory);

    function setUp() public {
        vm.startPrank(initialSupplyHolder);
        token = IERC20(rawCreate(compile()));
        vm.stopPrank();
    }

    function signPermit(
        IERC20Permit token_,
        uint256 privateKey,
        address owner,
        address spender,
        uint256 amount,
        uint256 deadline
    ) internal view returns (uint8 v, bytes32 r, bytes32 s) {
        bytes32 structHash = keccak256(
            abi.encode(PERMIT_TYPEHASH, owner, spender, amount, token_.nonces(owner), deadline)
        );
        bytes32 digest = keccak256(abi.encodePacked("\x19\x01", token_.DOMAIN_SEPARATOR(), structHash));
        return vm.sign(privateKey, digest);
    }

    function test_deploy() public {
        bytes memory initcode = compile();
        vm.snapshotValue(group(), "erc20.initcode_size", initcode.length);
        (bool success, bytes memory data) = CREATE2_FACTORY.call(bytes.concat(bytes32(uint256(0)), initcode));
        vm.snapshotGasLastCall(group(), "erc20.deploy");
        assertTrue(success);
        assertEq(data.length, 20);
        address deployed = address(bytes20(data));
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
        vm.snapshotGasLastCall(group(), "erc20.balanceOf");
    }

    function test_approve() public {
        address user = makeAddr("user");
        address spender = makeAddr("spender");

        vm.prank(user);
        token.approve(spender, 1000);
        vm.snapshotGasLastCall(group(), "erc20.approve");
    }

    function test_transferFrom() public {
        address spender = makeAddr("spender");

        vm.prank(initialSupplyHolder);
        token.approve(spender, type(uint256).max);

        vm.prank(spender);
        token.transferFrom(initialSupplyHolder, spender, 1000);
        vm.snapshotGasLastCall(group(), "erc20.transferFrom");
    }

    function test_permit() public {
        IERC20Permit token_ = IERC20Permit(address(token));
        uint256 privateKey = 0xA11CE;
        address owner = vm.addr(privateKey);
        address spender = makeAddr("spender");
        uint256 amount = 1000;
        uint256 deadline = block.timestamp;

        (uint8 v, bytes32 r, bytes32 s) = signPermit(token_, privateKey, owner, spender, amount, deadline);
        token_.permit(owner, spender, amount, deadline, v, r, s);
        vm.snapshotGasLastCall(group(), "erc20.permit");
    }
}

/// forge-config: default.isolate = true
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

/// forge-config: default.isolate = true
contract ERC20Solady is ERC20BenchmarkBase {
    function group() internal pure override returns (string memory) {
        return "solady";
    }

    function compile() internal pure override returns (bytes memory) {
        return type(ERC20).creationCode;
    }
}
