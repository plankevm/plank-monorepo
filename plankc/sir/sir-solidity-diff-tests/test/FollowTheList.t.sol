// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "./BaseTest.sol";
import {FollowTheList} from "src/FollowTheList.sol";
import {console} from "forge-std/console.sol";

/// @author philogy <https://github.com/philogy>
contract FollowTheListTest is BaseTest {
    FollowTheList solRef = new FollowTheList();
    address sirImpl = makeAddr("sir-implementation");

    function setUp() public {
        bytes memory sirCode = sir(abi.encode("src/follow_the_list.sir", "--init-only"));
        vm.etch(sirImpl, sirCode);
    }

    function test_simpleTermination() public {
        uint256[] memory nums = new uint256[](2);
        nums[0] = 1;
        nums[1] = 2;

        bytes memory dataIn = abi.encode(nums);
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_jumpOutOfBounds() public {
        console.logBytes(sirImpl.code);

        uint256[] memory nums = new uint256[](3);
        nums[0] = 1;
        nums[1] = 3;
        nums[2] = 0;

        bytes memory dataIn = abi.encode(nums);
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_emptyArray() public {
        uint256[] memory nums = new uint256[](0);

        bytes memory dataIn = abi.encode(nums);
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_singleElementOutOfBounds() public {
        uint256[] memory nums = new uint256[](1);
        nums[0] = 1;

        bytes memory dataIn = abi.encode(nums);
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_immediateOutOfBounds() public {
        uint256[] memory nums = new uint256[](3);
        nums[0] = 5;
        nums[1] = 1;
        nums[2] = 2;

        bytes memory dataIn = abi.encode(nums);
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }
}
