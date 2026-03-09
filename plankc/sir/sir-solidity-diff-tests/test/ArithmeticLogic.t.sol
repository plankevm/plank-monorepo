// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "./BaseTest.sol";
import {ArithmeticLogic} from "src/ArithmeticLogic.sol";

/// @author philogy <https://github.com/philogy>
contract ArithmeticLogicTest is BaseTest {
    ArithmeticLogic solRef = new ArithmeticLogic();
    address sirImpl = makeAddr("sir-implementation");

    function setUp() public {
        bytes memory sirInitcode = sir(abi.encode("src/arithmetic_logic.sir"));
        (bool initSucc,) = deployCodeTo(sirImpl, sirInitcode);
        assertTrue(initSucc, "sir init failed");
    }

    // Two-argument functions
    function test_fuzzing_add(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.add_, (a, b));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_mul(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.mul_, (a, b));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_sub(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.sub_, (a, b));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_div(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.div_, (a, b));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_sdiv(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.sdiv_, (a, b));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_mod(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.mod_, (a, b));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_smod(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.smod_, (a, b));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_exp(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.exp_, (a, b));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_signextend(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.signextend_, (a, b));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_lt(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.lt_, (a, b));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_gt(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.gt_, (a, b));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_slt(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.slt_, (a, b));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_sgt(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.sgt_, (a, b));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_eq(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.eq_, (a, b));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_and(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.and_, (a, b));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_or(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.or_, (a, b));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_xor(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.xor_, (a, b));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_byte(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.byte_, (a, b));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_shl(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.shl_, (a, b));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_shr(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.shr_, (a, b));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_sar(uint256 a, uint256 b) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.sar_, (a, b));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    // One-argument functions
    function test_fuzzing_iszero(uint256 a) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.iszero_, (a));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_not(uint256 a) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.not_, (a));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    // Three-argument functions
    function test_fuzzing_addmod(uint256 a, uint256 b, uint256 n) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.addmod_, (a, b, n));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }

    function test_fuzzing_mulmod(uint256 a, uint256 b, uint256 n) public {
        bytes memory dataIn = abi.encodeCall(ArithmeticLogic.mulmod_, (a, b, n));
        (bool refSucc, bytes memory refOut) = address(solRef).call(dataIn);
        (bool sirSucc, bytes memory sirOut) = sirImpl.call(dataIn);

        assertEq(refSucc, sirSucc, "different success");
        assertEq(refOut, sirOut, "different output data");
    }
}
