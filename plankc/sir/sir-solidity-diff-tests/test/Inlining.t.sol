// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "./BaseTest.sol";

contract InliningTest is BaseTest {
    function test_inlining() public {
        bytes memory codeWithout = sir(abi.encode("src/inlining.sir", "--init-only"));
        bytes memory codeWith = sir(abi.encode("src/inlining.sir", "--init-only", "--passes", "i"));

        address implWithout = makeAddr("without-inlining");
        address implWith = makeAddr("with-inlining");
        vm.etch(implWithout, codeWithout);
        vm.etch(implWith, codeWith);

        assertInliningResult(implWithout, implWith, 0, 7, 0);
        assertInliningResult(implWithout, implWith, 1, 7, 14);
    }

    function assertInliningResult(
        address implWithout,
        address implWith,
        uint256 selector,
        uint256 value,
        uint256 expected
    ) internal {
        bytes memory input = abi.encode(selector, value);
        (bool successWithout, bytes memory outputWithout) = implWithout.call(input);
        (bool successWith, bytes memory outputWith) = implWith.call(input);

        assertEq(successWithout, successWith, "inlining changed call success");
        assertEq(outputWithout, outputWith, "inlining changed output");
        assertTrue(successWith, "inlined call failed");
        assertEq(outputWith, abi.encode(expected), "unexpected output");
    }
}
