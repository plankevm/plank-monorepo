// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {BaseTest} from "test/BaseTest.sol";

abstract contract ClzBenchmarkBase is BaseTest {
    address impl;

    function group() internal pure returns (string memory) {
        return "bit";
    }

    function key() internal pure virtual returns (string memory);
    function compile() internal virtual returns (bytes memory);

    function setUp() public {
        impl = makeAddr("clz-bench-impl");
        vm.etch(impl, compile());
    }

    function test_clz() public {
        (bool ok,) = impl.call(abi.encode(uint256(1) << 127));
        vm.snapshotGasLastCall(group(), key());
        assertTrue(ok, "clz call reverted");
    }
}

/// forge-config: default.isolate = true
contract ClzFallback is ClzBenchmarkBase {
    function key() internal pure override returns (string memory) {
        return "clz.fallback";
    }

    function compile() internal override returns (bytes memory) {
        return plankBuild(
            "src/std/bit_test.plk",
            baseBuildOptions().withBackend("sir-release").withOptimizations("csud").withEvmVersion("prague")
        );
    }
}

/// forge-config: default.isolate = true
contract ClzNative is ClzBenchmarkBase {
    function key() internal pure override returns (string memory) {
        return "clz.native";
    }

    function compile() internal override returns (bytes memory) {
        return plankBuild(
            "src/std/bit_test.plk",
            baseBuildOptions().withBackend("sir-release").withOptimizations("csud").withEvmVersion("osaka")
        );
    }
}
